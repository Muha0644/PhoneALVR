package dev.phonealvr.client

import android.content.Context
import org.json.JSONObject

data class AppSettings(
  val brightness: Float = 1f,
  val vignette: Float = 0f,
  val chromaticAberration: Float = 0f,
  val refreshRateHz: Float? = null,
  val resolutionCapWidth: Int = 0,
  val resolutionCapHeight: Int = 0,
  val debugOverlay: Boolean = false,
  val preferWired: Boolean = false,
  val crashReporting: Boolean = false,
  val crashReportingEndpoint: String = "",
  val serverHint: String = "",
  val viewerProfile: String? = null,
) {
  fun toNativeJson(): String {
    val root = JSONObject()
    root.put("brightness", brightness.toDouble())
    root.put("vignette", vignette.toDouble())
    root.put("chromatic_aberration", chromaticAberration.toDouble())
    root.put("refresh_rate_hz", refreshRateHz?.toDouble() ?: JSONObject.NULL)
    if (resolutionCapWidth > 0 && resolutionCapHeight > 0) {
      root.put(
        "resolution_cap",
        JSONObject().put("width", resolutionCapWidth).put("height", resolutionCapHeight),
      )
    } else {
      root.put("resolution_cap", JSONObject.NULL)
    }
    root.put("debug_overlay", debugOverlay)
    root.put("prefer_wired", preferWired)
    root.put("crash_reporting", crashReporting)
    root.put(
      "crash_reporting_endpoint",
      crashReportingEndpoint.ifBlank { null } ?: JSONObject.NULL,
    )
    root.put("server", serverHint.ifBlank { null } ?: JSONObject.NULL)
    root.put("viewer_profile", viewerProfile ?: JSONObject.NULL)
    return root.toString()
  }
}

object SettingsStore {
  private const val PREFS = "phonealvr_settings"
  private const val KEY_BRIGHTNESS = "brightness"
  private const val KEY_VIGNETTE = "vignette"
  private const val KEY_CA = "chromatic_aberration"
  private const val KEY_REFRESH = "refresh_rate_hz"
  private const val KEY_CAP_WIDTH = "resolution_cap_width"
  private const val KEY_CAP_HEIGHT = "resolution_cap_height"
  private const val KEY_DEBUG = "debug_overlay"
  private const val KEY_WIRED = "prefer_wired"
  private const val KEY_CRASH = "crash_reporting"
  private const val KEY_CRASH_ENDPOINT = "crash_reporting_endpoint"
  private const val KEY_SERVER = "server_hint"
  private const val KEY_VIEWER = "viewer_profile"

  fun load(context: Context): AppSettings {
    val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    return AppSettings(
      brightness = prefs.getFloat(KEY_BRIGHTNESS, 1f),
      vignette = prefs.getFloat(KEY_VIGNETTE, 0f),
      chromaticAberration = prefs.getFloat(KEY_CA, 0f),
      refreshRateHz = if (prefs.contains(KEY_REFRESH)) prefs.getFloat(KEY_REFRESH, 0f) else null,
      resolutionCapWidth = prefs.getInt(KEY_CAP_WIDTH, 0),
      resolutionCapHeight = prefs.getInt(KEY_CAP_HEIGHT, 0),
      debugOverlay = prefs.getBoolean(KEY_DEBUG, false),
      preferWired = prefs.getBoolean(KEY_WIRED, false),
      crashReporting = prefs.getBoolean(KEY_CRASH, false),
      crashReportingEndpoint = prefs.getString(KEY_CRASH_ENDPOINT, "") ?: "",
      serverHint = prefs.getString(KEY_SERVER, "") ?: "",
      viewerProfile = prefs.getString(KEY_VIEWER, null),
    )
  }

  fun save(context: Context, settings: AppSettings) {
    context
      .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
      .edit()
      .putFloat(KEY_BRIGHTNESS, settings.brightness)
      .putFloat(KEY_VIGNETTE, settings.vignette)
      .putFloat(KEY_CA, settings.chromaticAberration)
      .apply {
        if (settings.refreshRateHz != null) {
          putFloat(KEY_REFRESH, settings.refreshRateHz)
        } else {
          remove(KEY_REFRESH)
        }
      }
      .putInt(KEY_CAP_WIDTH, settings.resolutionCapWidth)
      .putInt(KEY_CAP_HEIGHT, settings.resolutionCapHeight)
      .putBoolean(KEY_DEBUG, settings.debugOverlay)
      .putBoolean(KEY_WIRED, settings.preferWired)
      .putBoolean(KEY_CRASH, settings.crashReporting)
      .putString(KEY_CRASH_ENDPOINT, settings.crashReportingEndpoint)
      .putString(KEY_SERVER, settings.serverHint)
      .putString(KEY_VIEWER, settings.viewerProfile)
      .apply()
  }

  fun applyToNative(settings: AppSettings): Boolean {
    return NativeSession.nativeUpdateSettings(settings.toNativeJson())
  }

  fun saveAndApply(context: Context, settings: AppSettings): Boolean {
    save(context, settings)
    return applyToNative(settings)
  }

  fun applyViewerProfile(context: Context, payload: String): Boolean {
    val settings = load(context).copy(viewerProfile = payload)
    save(context, settings)
    return NativeSession.nativeSetViewerProfile(payload)
  }

  fun clearViewerProfile(context: Context) {
    save(context, load(context).copy(viewerProfile = null))
  }
}
