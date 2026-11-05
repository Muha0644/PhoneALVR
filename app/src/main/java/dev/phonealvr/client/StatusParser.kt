package dev.phonealvr.client

import org.json.JSONObject

data class StreamUi(
  val width: Int,
  val height: Int,
  val refreshHz: Float,
  val protocol: Int,
  val foveated: Boolean = false,
  val encodedWidth: Int = 0,
  val encodedHeight: Int = 0,
)

data class StatsUi(
  val fps: Float = 0f,
  val frameTimeMs: Float = 0f,
  val latencyMs: Float = 0f,
  val droppedFrames: Long = 0,
  val frames: Long = 0,
  val bitrateMbps: Float = 0f,
  val viewWidth: Int = 0,
  val viewHeight: Int = 0,
  val codec: String = "unknown",
  val trackingHz: Float = 0f,
)

data class SessionUiState(
  val state: String = "idle",
  val label: String = "Idle",
  val peer: String? = null,
  val stream: StreamUi? = null,
  val error: String? = null,
  val hudMessage: String? = null,
  val generation: Long = 0,
  val stats: StatsUi = StatsUi(),
) {
  val isListening: Boolean
    get() =
      state == "advertising" ||
        state == "incoming_connection" ||
        state == "handshaking" ||
        state == "streaming" ||
        state == "reconnecting"

  val isStreaming: Boolean
    get() = state == "streaming"
}

object StatusParser {
  fun parseStatus(json: String): SessionUiState {
    val root = runCatching { JSONObject(json) }.getOrNull() ?: return SessionUiState()
    val stream =
      root.optJSONObject("stream")?.let {
        StreamUi(
          width = it.optInt("view_width"),
          height = it.optInt("view_height"),
          refreshHz = it.optDouble("refresh_hz", 0.0).toFloat(),
          protocol = it.optInt("protocol"),
          foveated = it.optBoolean("foveated", false),
          encodedWidth = it.optInt("encoded_view_width"),
          encodedHeight = it.optInt("encoded_view_height"),
        )
      }
    return SessionUiState(
      state = root.optString("state", "idle"),
      label = root.optString("state_label", root.optString("state", "Idle")),
      peer = root.optString("peer").ifBlank { null },
      stream = stream,
      error = root.optString("last_error").ifBlank { null },
      hudMessage = root.optString("hud_message").ifBlank { null },
      generation = root.optLong("generation"),
    )
  }

  fun parseStats(json: String): StatsUi {
    val root = runCatching { JSONObject(json) }.getOrNull() ?: return StatsUi()
    return StatsUi(
      fps = root.optDouble("fps", 0.0).toFloat(),
      frameTimeMs = root.optDouble("frame_time_ms", 0.0).toFloat(),
      latencyMs = root.optDouble("latency_ms", 0.0).toFloat(),
      droppedFrames = root.optLong("dropped_frames"),
      frames = root.optLong("frames"),
      bitrateMbps = root.optDouble("bitrate_mbps", 0.0).toFloat(),
      viewWidth = root.optInt("view_width"),
      viewHeight = root.optInt("view_height"),
      codec = root.optString("codec", "unknown"),
      trackingHz = root.optDouble("tracking_hz", 0.0).toFloat(),
    )
  }
}
