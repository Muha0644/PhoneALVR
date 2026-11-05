//! Explicit session state machine.
//!
//! ALVR is unusual: the PC streamer is the TCP client and the phone is the
//! listener. This state machine therefore never models "dial the server". It
//! models the lifecycle from an idle listener to an accepted, handshaken and
//! streaming session, and keeps the listener alive across disconnects so the
//! streamer can reconnect without restarting the app.

use serde::{Deserialize, Serialize};
use std::fmt;

/// States are intentionally explicit. Adding a state requires updating
/// [`Session::handle`] and the transition tests in this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// No listener, no session.
    Idle,
    /// Listening and advertising; waiting for an ALVR streamer to connect.
    Advertising,
    /// A streamer accepted the TCP connection; its handshake is pending.
    IncomingConnection,
    /// The protocol handshake is in progress.
    Handshaking,
    /// Video and tracking are running.
    Streaming,
    /// The stream was lost; the listener remains open for a reconnect.
    Reconnecting,
    /// Destroyed. `reset()` is required before starting again.
    Closed,
}

impl SessionState {
    pub fn is_listening(self) -> bool {
        matches!(
            self,
            Self::Advertising
                | Self::IncomingConnection
                | Self::Handshaking
                | Self::Streaming
                | Self::Reconnecting
        )
    }

    pub fn is_streaming(self) -> bool {
        self == Self::Streaming
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Advertising => "waiting_for_alvr",
            Self::IncomingConnection => "incoming_connection",
            Self::Handshaking => "handshaking",
            Self::Streaming => "streaming",
            Self::Reconnecting => "reconnecting",
            Self::Closed => "closed",
        }
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Negotiated stream properties. The encoded resolution is not necessarily
/// the requested resolution: ALVR aligns it down to a multiple of 32.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StreamInfo {
    pub view_width: u32,
    pub view_height: u32,
    pub refresh_hz: f32,
    pub protocol: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// User (or service) starts waiting.
    Start,
    /// Stop waiting but keep the object reusable.
    Stop,
    /// Destroy the session. Idempotent.
    Close,
    /// Leave `Closed` and return to `Idle`.
    Reset,
    /// The listener accepted a streamer.
    ServerConnected {
        peer: String,
    },
    /// The streamer sent its first handshake packet.
    HandshakeStarted,
    HandshakeSucceeded {
        protocol: u32,
    },
    HandshakeFailed {
        reason: String,
    },
    StreamStarted {
        view_width: u32,
        view_height: u32,
        refresh_hz: f32,
        protocol: u32,
    },
    StreamStopped,
    NetworkLost,
    NetworkRestored,
    SurfaceAvailable,
    SurfaceLost,
    Error(String),
}

/// Side effects the platform layer should perform. The state machine is pure;
/// it never touches sockets, the decoder, EGL or JNI itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    StartListener,
    StopListener,
    AwaitHandshake,
    StopStream,
    PauseRender,
    ResumeRender,
    ReportError(String),
    StateChanged(SessionState),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionStatus {
    pub state: SessionState,
    pub state_label: String,
    pub peer: Option<String>,
    pub stream: Option<StreamInfo>,
    pub last_error: Option<String>,
    /// Increments on every state change. Lets the UI drop stale snapshots.
    pub generation: u64,
}

pub struct Session {
    state: SessionState,
    peer: Option<String>,
    stream: Option<StreamInfo>,
    last_error: Option<String>,
    generation: u64,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Self {
            state: SessionState::Idle,
            peer: None,
            stream: None,
            last_error: None,
            generation: 0,
        }
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn status(&self) -> SessionStatus {
        SessionStatus {
            state: self.state,
            state_label: self.state.label().to_owned(),
            peer: self.peer.clone(),
            stream: self.stream,
            last_error: self.last_error.clone(),
            generation: self.generation,
        }
    }

    pub fn status_json(&self) -> String {
        serde_json::to_string(&self.status()).unwrap_or_else(|_| {
            r#"{"state":"closed","state_label":"closed","generation":0}"#.to_owned()
        })
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Apply one event and return the side effects, in execution order.
    pub fn handle(&mut self, event: Event) -> Vec<Action> {
        use Event as E;
        use SessionState as S;

        let previous = self.state;
        let mut actions = Vec::new();

        match event {
            E::Start => match previous {
                S::Idle => {
                    self.state = S::Advertising;
                    self.last_error = None;
                    actions.push(Action::StartListener);
                }
                S::Closed => {
                    self.note_error("session is closed; reset it before starting");
                }
                _ => {}
            },

            E::Stop => {
                if previous != S::Idle && previous != S::Closed {
                    if previous == S::Streaming {
                        actions.push(Action::StopStream);
                    }
                    actions.push(Action::StopListener);
                    self.state = S::Idle;
                }
            }

            E::Close => {
                if previous != S::Closed {
                    if previous == S::Streaming {
                        actions.push(Action::StopStream);
                    }
                    if previous.is_listening() {
                        actions.push(Action::StopListener);
                    }
                    self.state = S::Closed;
                }
            }

            E::Reset => {
                if previous == S::Closed {
                    self.state = S::Idle;
                    self.peer = None;
                    self.stream = None;
                    self.last_error = None;
                }
            }

            E::ServerConnected { peer } => match previous {
                S::Advertising | S::Reconnecting => {
                    self.peer = Some(peer);
                    self.state = S::IncomingConnection;
                    actions.push(Action::AwaitHandshake);
                }
                _ => {
                    // A duplicate accept (for example a race between the
                    // control and stream sockets) must not restart anything.
                }
            },

            E::HandshakeStarted => {
                if previous == S::IncomingConnection {
                    self.state = S::Handshaking;
                }
            }

            E::HandshakeSucceeded { protocol } => {
                if matches!(previous, S::IncomingConnection | S::Handshaking) {
                    self.state = S::Handshaking;
                    if let Some(stream) = &mut self.stream {
                        stream.protocol = protocol;
                    }
                }
            }

            E::HandshakeFailed { reason } => {
                if matches!(
                    previous,
                    S::IncomingConnection | S::Handshaking | S::Reconnecting
                ) {
                    self.note_error(reason.clone());
                    self.state = S::Advertising;
                    actions.push(Action::ReportError(reason));
                }
            }

            E::StreamStarted {
                view_width,
                view_height,
                refresh_hz,
                protocol,
            } => {
                if matches!(
                    previous,
                    S::Advertising
                        | S::IncomingConnection
                        | S::Handshaking
                        | S::Streaming
                        | S::Reconnecting
                ) {
                    self.stream = Some(StreamInfo {
                        view_width,
                        view_height,
                        refresh_hz,
                        protocol,
                    });
                    self.state = S::Streaming;
                    actions.push(Action::ResumeRender);
                }
            }

            E::StreamStopped => {
                if matches!(previous, S::Streaming | S::Reconnecting) {
                    self.stream = None;
                    self.state = S::Advertising;
                    actions.push(Action::StopStream);
                }
            }

            E::NetworkLost => match previous {
                S::Streaming => {
                    self.stream = None;
                    self.state = S::Reconnecting;
                    actions.push(Action::StopStream);
                }
                S::IncomingConnection | S::Handshaking => {
                    self.state = S::Advertising;
                }
                _ => {}
            },

            E::NetworkRestored => {
                if previous == S::Reconnecting {
                    self.state = S::Advertising;
                }
            }

            E::SurfaceAvailable => {
                if previous == S::Streaming {
                    actions.push(Action::ResumeRender);
                }
            }

            E::SurfaceLost => {
                if previous == S::Streaming {
                    actions.push(Action::PauseRender);
                }
            }

            E::Error(message) => {
                self.note_error(message.clone());
                actions.push(Action::ReportError(message));
                if matches!(
                    previous,
                    S::IncomingConnection | S::Handshaking | S::Streaming | S::Reconnecting
                ) {
                    if previous == S::Streaming {
                        actions.push(Action::StopStream);
                    }
                    self.state = S::Reconnecting;
                }
            }
        }

        if self.state != previous {
            self.generation = self.generation.wrapping_add(1);
            actions.push(Action::StateChanged(self.state));
        }

        actions
    }

    fn note_error(&mut self, message: impl Into<String>) {
        self.last_error = Some(message.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(session: &mut Session, events: &[Event]) -> Vec<Action> {
        events
            .iter()
            .flat_map(|e| session.handle(e.clone()))
            .collect()
    }

    fn connected_streaming() -> Session {
        let mut session = Session::new();
        drive(
            &mut session,
            &[
                Event::Start,
                Event::ServerConnected {
                    peer: "192.0.2.10:50000".into(),
                },
                Event::HandshakeStarted,
                Event::HandshakeSucceeded { protocol: 20 },
                Event::StreamStarted {
                    view_width: 1184,
                    view_height: 1056,
                    refresh_hz: 90.0,
                    protocol: 20,
                },
            ],
        );
        session
    }

    #[test]
    fn happy_path_reaches_streaming() {
        let mut session = Session::new();
        let actions = drive(
            &mut session,
            &[
                Event::Start,
                Event::ServerConnected {
                    peer: "192.0.2.10:50000".into(),
                },
                Event::HandshakeStarted,
                Event::HandshakeSucceeded { protocol: 20 },
                Event::StreamStarted {
                    view_width: 1184,
                    view_height: 1056,
                    refresh_hz: 90.0,
                    protocol: 20,
                },
            ],
        );

        assert_eq!(session.state(), SessionState::Streaming);
        assert!(actions.contains(&Action::StartListener));
        assert!(actions.contains(&Action::AwaitHandshake));
        assert!(actions.contains(&Action::ResumeRender));
        let status = session.status();
        assert_eq!(status.peer.as_deref(), Some("192.0.2.10:50000"));
        assert_eq!(status.stream.unwrap().view_width, 1184);
    }

    #[test]
    fn stop_is_idempotent_and_returns_to_idle() {
        let mut session = connected_streaming();
        let first = session.handle(Event::Stop);
        assert_eq!(session.state(), SessionState::Idle);
        assert!(first.contains(&Action::StopStream));
        assert!(first.contains(&Action::StopListener));

        let second = session.handle(Event::Stop);
        assert!(second.is_empty());
        assert_eq!(session.state(), SessionState::Idle);
    }

    #[test]
    fn close_is_idempotent_from_every_state() {
        let states: Vec<Vec<Event>> = vec![
            vec![],
            vec![Event::Start],
            vec![Event::Start, Event::ServerConnected { peer: "p".into() }],
            vec![
                Event::Start,
                Event::ServerConnected { peer: "p".into() },
                Event::HandshakeStarted,
            ],
            vec![
                Event::Start,
                Event::ServerConnected { peer: "p".into() },
                Event::HandshakeStarted,
                Event::StreamStarted {
                    view_width: 1,
                    view_height: 1,
                    refresh_hz: 60.0,
                    protocol: 20,
                },
            ],
        ];

        for setup in states {
            let mut session = Session::new();
            drive(&mut session, &setup);
            session.handle(Event::Close);
            assert_eq!(session.state(), SessionState::Closed);
            assert!(session.handle(Event::Close).is_empty());
        }
    }

    #[test]
    fn one_hundred_start_stop_cycles_do_not_corrupt_state() {
        let mut session = Session::new();
        for cycle in 0..100 {
            session.handle(Event::Start);
            assert_eq!(session.state(), SessionState::Advertising, "cycle {cycle}");
            session.handle(Event::ServerConnected { peer: "p".into() });
            session.handle(Event::HandshakeStarted);
            session.handle(Event::HandshakeSucceeded { protocol: 20 });
            session.handle(Event::StreamStarted {
                view_width: 1184,
                view_height: 1056,
                refresh_hz: 90.0,
                protocol: 20,
            });
            assert_eq!(session.state(), SessionState::Streaming);
            session.handle(Event::StreamStopped);
            assert_eq!(session.state(), SessionState::Advertising);
            session.handle(Event::Stop);
            assert_eq!(session.state(), SessionState::Idle);
        }
        assert_eq!(session.status().generation, 100 * 6);
    }

    #[test]
    fn disconnect_keeps_listener_and_reconnects() {
        let mut session = connected_streaming();
        let actions = session.handle(Event::NetworkLost);
        assert_eq!(session.state(), SessionState::Reconnecting);
        assert!(actions.contains(&Action::StopStream));

        session.handle(Event::NetworkRestored);
        assert_eq!(session.state(), SessionState::Advertising);

        session.handle(Event::ServerConnected { peer: "p2".into() });
        assert_eq!(session.state(), SessionState::IncomingConnection);
    }

    #[test]
    fn stream_can_restart_while_reconnecting() {
        let mut session = connected_streaming();
        session.handle(Event::NetworkLost);
        assert_eq!(session.state(), SessionState::Reconnecting);
        session.handle(Event::StreamStarted {
            view_width: 1184,
            view_height: 1056,
            refresh_hz: 90.0,
            protocol: 20,
        });
        assert_eq!(session.state(), SessionState::Streaming);
    }

    #[test]
    fn stream_stopped_returns_to_advertising() {
        let mut session = connected_streaming();
        let actions = session.handle(Event::StreamStopped);
        assert_eq!(session.state(), SessionState::Advertising);
        assert!(actions.contains(&Action::StopStream));
        assert!(session.status().stream.is_none());
    }

    #[test]
    fn handshake_failure_reports_and_keeps_listening() {
        let mut session = Session::new();
        drive(
            &mut session,
            &[
                Event::Start,
                Event::ServerConnected { peer: "p".into() },
                Event::HandshakeFailed {
                    reason: "protocol mismatch".into(),
                },
            ],
        );
        assert_eq!(session.state(), SessionState::Advertising);
        assert_eq!(session.last_error(), Some("protocol mismatch"));
    }

    #[test]
    fn duplicate_accept_is_ignored() {
        let mut session = Session::new();
        session.handle(Event::Start);
        session.handle(Event::ServerConnected { peer: "a".into() });
        let actions = session.handle(Event::ServerConnected { peer: "b".into() });
        assert!(actions.is_empty());
        assert_eq!(session.state(), SessionState::IncomingConnection);
        assert_eq!(session.status().peer.as_deref(), Some("a"));
    }

    #[test]
    fn surface_events_pause_and_resume_without_leaving_streaming() {
        let mut session = connected_streaming();
        let lost = session.handle(Event::SurfaceLost);
        assert!(lost.contains(&Action::PauseRender));
        assert_eq!(session.state(), SessionState::Streaming);
        let available = session.handle(Event::SurfaceAvailable);
        assert!(available.contains(&Action::ResumeRender));
    }

    #[test]
    fn errors_during_streaming_enter_reconnecting() {
        let mut session = connected_streaming();
        let actions = session.handle(Event::Error("decoder died".into()));
        assert_eq!(session.state(), SessionState::Reconnecting);
        assert!(actions.contains(&Action::StopStream));
        assert!(actions.contains(&Action::ReportError("decoder died".into())));
    }

    #[test]
    fn closed_session_can_be_reset_and_reused() {
        let mut session = connected_streaming();
        session.handle(Event::Close);
        session.handle(Event::Reset);
        assert_eq!(session.state(), SessionState::Idle);
        session.handle(Event::Start);
        assert_eq!(session.state(), SessionState::Advertising);
    }
}
