package dev.phonealvr.client

import android.content.pm.ActivityInfo
import org.junit.Assert.assertEquals
import org.junit.Test

class ScreenOrientationTest {
  @Test
  fun autoStaysLandscapeInVr() {
    assertEquals(
      ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE,
      ScreenOrientation.AUTO.activityOrientation(vr = true),
    )
  }

  @Test
  fun autoFollowsSensorOutsideVr() {
    assertEquals(
      ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED,
      ScreenOrientation.AUTO.activityOrientation(vr = false),
    )
  }

  @Test
  fun fixedOrientationsIgnoreVrFlag() {
    assertEquals(
      ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE,
      ScreenOrientation.LANDSCAPE.activityOrientation(vr = false),
    )
    assertEquals(
      ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE,
      ScreenOrientation.LANDSCAPE.activityOrientation(vr = true),
    )
    assertEquals(
      ActivityInfo.SCREEN_ORIENTATION_REVERSE_LANDSCAPE,
      ScreenOrientation.REVERSE_LANDSCAPE.activityOrientation(vr = true),
    )
    assertEquals(
      ActivityInfo.SCREEN_ORIENTATION_PORTRAIT,
      ScreenOrientation.PORTRAIT.activityOrientation(vr = true),
    )
    assertEquals(
      ActivityInfo.SCREEN_ORIENTATION_REVERSE_PORTRAIT,
      ScreenOrientation.REVERSE_PORTRAIT.activityOrientation(vr = false),
    )
  }
}
