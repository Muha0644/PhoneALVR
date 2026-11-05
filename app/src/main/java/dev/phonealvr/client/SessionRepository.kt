package dev.phonealvr.client

import android.content.Context
import android.util.DisplayMetrics
import android.view.WindowManager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * Process-wide session facade. The native controller owns the actual state; this object polls it
 * off the UI thread and exposes a StateFlow to Compose.
 */
object SessionRepository {
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  private val _state = MutableStateFlow(SessionUiState())
  val state: StateFlow<SessionUiState> = _state.asStateFlow()

  @Volatile private var created = false
  private var pollJob: Job? = null

  fun startPolling() {
    if (pollJob?.isActive == true) return
    pollJob =
      scope.launch {
        while (isActive) {
          refresh()
          delay(250)
        }
      }
  }

  fun stopPolling() {
    pollJob?.cancel()
    pollJob = null
  }

  fun refresh() {
    if (!NativeSession.load()) {
      _state.value =
        SessionUiState(
          state = "error",
          label = "Native library missing",
          error = "libphonealvr.so could not be loaded",
        )
      return
    }
    val status = runCatching { NativeSession.nativePollStatus() }.getOrDefault("{}")
    val stats = runCatching { NativeSession.nativeGetStats() }.getOrDefault("{}")
    val parsed = StatusParser.parseStatus(status)
    _state.value = parsed.copy(stats = StatusParser.parseStats(stats))
  }

  /** Create the native session and start advertising. Idempotent. */
  fun start(context: Context): Boolean {
    if (!NativeSession.load()) {
      _state.value =
        SessionUiState(
          state = "error",
          label = "Native library missing",
          error = "libphonealvr.so could not be loaded",
        )
      return false
    }

    if (!created) {
      val windowManager = context.getSystemService(Context.WINDOW_SERVICE) as WindowManager
      @Suppress("DEPRECATION") val display = windowManager.defaultDisplay
      val metrics = DisplayMetrics()
      @Suppress("DEPRECATION") display.getRealMetrics(metrics)
      val refresh = display.refreshRate.takeIf { it.isFinite() && it > 1f } ?: 60f
      val initialSettings = SettingsStore.load(context)
      created =
        NativeSession.nativeCreate(
          metrics.widthPixels,
          metrics.heightPixels,
          refresh,
          AlvrContext(context.applicationContext),
          initialSettings.toNativeJson(),
        )
      if (!created) {
        _state.value =
          SessionUiState(
            state = "error",
            label = "Could not create session",
            error = "nativeCreate failed; check logcat tag PhoneALVR",
          )
        return false
      }
    }

    SettingsStore.saveAndApply(context, SettingsStore.load(context))
    NativeSession.nativeStart()
    startPolling()
    refresh()
    return true
  }

  fun stop() {
    if (!created) return
    NativeSession.nativeStop()
    refresh()
  }

  fun destroy() {
    stopPolling()
    if (created) {
      NativeSession.nativeStop()
      NativeSession.nativeDestroy()
      created = false
    }
    _state.value = SessionUiState()
  }

  fun recenter() {
    if (created) NativeSession.nativeRecenter()
  }

  fun isCreated(): Boolean = created
}
