package dev.phonealvr.client

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class StatusParserTest {
  @Test
  fun parsesAdvertisingStatus() {
    val state =
      StatusParser.parseStatus(
        """{"state":"advertising","state_label":"waiting_for_alvr","generation":3}""",
      )
    assertEquals("advertising", state.state)
    assertEquals("waiting_for_alvr", state.label)
    assertTrue(state.isListening)
    assertNull(state.peer)
    assertEquals(3, state.generation)
  }

  @Test
  fun parsesStreamingStatusWithStream() {
    val state =
      StatusParser.parseStatus(
        """
                {"state":"streaming","state_label":"streaming",
                 "peer":"192.0.2.10:50000",
                 "stream":{"view_width":1184,"view_height":1056,"refresh_hz":90.0,"protocol":20},
                 "generation":7}
                """
          .trimIndent(),
      )
    assertTrue(state.isStreaming)
    assertEquals("192.0.2.10:50000", state.peer)
    assertEquals(1184, state.stream?.width)
    assertEquals(1056, state.stream?.height)
    assertEquals(90f, state.stream?.refreshHz ?: 0f, 0.01f)
  }

  @Test
  fun parsesFoveatedStreamEncodedSize() {
    val state =
      StatusParser.parseStatus(
        """
                {"state":"streaming","state_label":"streaming",
                 "stream":{"view_width":1184,"view_height":1056,"refresh_hz":90.0,"protocol":20,
                           "foveated":true,"encoded_view_width":704,"encoded_view_height":672},
                 "generation":8}
                """
          .trimIndent(),
      )
    assertEquals(true, state.stream?.foveated)
    assertEquals(704, state.stream?.encodedWidth)
    assertEquals(672, state.stream?.encodedHeight)
  }

  @Test
  fun parsesStats() {
    val stats =
      StatusParser.parseStats(
        """
                {"fps":89.8,"frame_time_ms":11.1,"latency_ms":42.5,"dropped_frames":2,
                 "frames":100,"bitrate_mbps":80.0,"view_width":1184,"view_height":1056,
                 "codec":"h264","tracking_hz":500.0}
                """
          .trimIndent(),
      )
    assertEquals(89.8f, stats.fps, 0.01f)
    assertEquals(2, stats.droppedFrames)
    assertEquals("h264", stats.codec)
    assertEquals(500f, stats.trackingHz, 0.01f)
  }

  @Test
  fun malformedJsonFallsBackToIdle() {
    assertEquals("idle", StatusParser.parseStatus("not json").state)
    assertEquals("idle", StatusParser.parseStatus("").state)
    assertEquals(0f, StatusParser.parseStats("{}").fps, 0.0f)
  }

  @Test
  fun hudMessageIsParsed() {
    val state =
      StatusParser.parseStatus(
        """{"state":"advertising","state_label":"waiting_for_alvr","hud_message":"Searching for streamer...","generation":1}""",
      )
    assertEquals("Searching for streamer...", state.hudMessage)
  }

  @Test
  fun errorIsExposed() {
    val state =
      StatusParser.parseStatus(
        """{"state":"advertising","state_label":"waiting_for_alvr",
                    "last_error":"protocol mismatch","generation":9}""",
      )
    assertEquals("protocol mismatch", state.error)
  }
}
