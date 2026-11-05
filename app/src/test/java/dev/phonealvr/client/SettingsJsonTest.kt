package dev.phonealvr.client

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SettingsJsonTest {
  @Test
  fun defaultsMatchNativeSchema() {
    val json = JSONObject(AppSettings().toNativeJson())
    assertEquals(1.0, json.getDouble("brightness"), 0.0)
    assertEquals(0.0, json.getDouble("vignette"), 0.0)
    assertEquals(0.0, json.getDouble("chromatic_aberration"), 0.0)
    assertTrue(json.isNull("refresh_rate_hz"))
    assertTrue(json.isNull("resolution_cap"))
    assertTrue(json.isNull("viewer_profile"))
    assertFalse(json.getBoolean("debug_overlay"))
  }

  @Test
  fun viewerProfileAndCapAreEncoded() {
    val settings =
      AppSettings(
        viewerProfile = "http://google.com/cardboard/cfg?p=abc",
        resolutionCapWidth = 1184,
        resolutionCapHeight = 1056,
        debugOverlay = true,
        refreshRateHz = 90f,
      )
    val json = JSONObject(settings.toNativeJson())
    assertEquals("http://google.com/cardboard/cfg?p=abc", json.getString("viewer_profile"))
    assertEquals(1184, json.getJSONObject("resolution_cap").getInt("width"))
    assertEquals(1056, json.getJSONObject("resolution_cap").getInt("height"))
    assertEquals(90.0, json.getDouble("refresh_rate_hz"), 0.0)
    assertTrue(json.getBoolean("debug_overlay"))
  }

  @Test
  fun incompleteCapFallsBackToNative() {
    val json =
      JSONObject(
        AppSettings(resolutionCapWidth = 1184, resolutionCapHeight = 0).toNativeJson(),
      )
    assertTrue(json.isNull("resolution_cap"))
  }
}
