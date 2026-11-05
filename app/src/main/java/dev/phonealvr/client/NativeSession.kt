package dev.phonealvr.client

import android.content.Context
import android.view.Surface

/**
 * Thin JNI facade. All policy, protocol and rendering logic lives in the Rust core; this class only
 * translates Kotlin types to the small native surface.
 */
class NativeSession private constructor() {
  companion object {
    private var loaded = false

    @JvmStatic
    @Synchronized
    fun load(): Boolean {
      if (loaded) return true
      loaded = runCatching { System.loadLibrary("phonealvr") }.isSuccess
      return loaded
    }

    @JvmStatic
    external fun nativeCreate(
      width: Int,
      height: Int,
      refreshHz: Float,
      xdpi: Float,
      context: Context,
      settingsJson: String,
    ): Boolean

    @JvmStatic external fun nativeDestroy()

    @JvmStatic external fun nativeStart()

    @JvmStatic external fun nativeStop()

    @JvmStatic external fun nativeUpdateSettings(json: String): Boolean

    @JvmStatic external fun nativeSetViewerProfile(json: String): Boolean

    @JvmStatic external fun nativeOnSurfaceCreated(surface: Surface): Boolean

    @JvmStatic external fun nativeOnSurfaceChanged(width: Int, height: Int)

    @JvmStatic external fun nativeOnSurfaceDestroyed()

    @JvmStatic external fun nativeRenderFrame()

    @JvmStatic external fun nativeRecenter()

    @JvmStatic
    external fun nativePushOrientation(
      x: Float,
      y: Float,
      z: Float,
      w: Float,
      timestampNs: Long,
      displayRotation: Int,
    )

    @JvmStatic external fun nativePollStatus(): String

    @JvmStatic external fun nativeGetStats(): String

    @JvmStatic external fun nativeGetDiagnostics(): String
  }
}
