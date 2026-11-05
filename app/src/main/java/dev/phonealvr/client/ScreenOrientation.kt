package dev.phonealvr.client

import android.content.pm.ActivityInfo

/** User-selected screen orientation lock for the app and the VR view. */
enum class ScreenOrientation(val label: String) {
  AUTO("Auto"),
  PORTRAIT("Portrait"),
  LANDSCAPE("Landscape"),
  REVERSE_PORTRAIT("Reverse portrait"),
  REVERSE_LANDSCAPE("Reverse landscape");

  /**
   * `ActivityInfo` constant for this choice. In VR, Auto keeps the existing sensor-driven landscape
   * behaviour instead of allowing portrait.
   */
  fun activityOrientation(vr: Boolean): Int =
    when (this) {
      AUTO ->
        if (vr) ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        else ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
      PORTRAIT -> ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
      LANDSCAPE -> ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
      REVERSE_PORTRAIT -> ActivityInfo.SCREEN_ORIENTATION_REVERSE_PORTRAIT
      REVERSE_LANDSCAPE -> ActivityInfo.SCREEN_ORIENTATION_REVERSE_LANDSCAPE
    }
}
