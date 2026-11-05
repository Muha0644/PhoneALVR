package dev.phonealvr.client

import android.content.res.Configuration
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Bundle
import android.util.Log
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.launch

/**
 * Full-screen VR activity. The native renderer owns EGL/GLES; this activity only supplies a Surface
 * and the IMU stream, and keeps the display awake.
 */
class VrActivity : ComponentActivity(), SurfaceHolder.Callback {
  companion object {
    private const val TAG = "PhoneALVR-Render"
  }

  private lateinit var surfaceView: SurfaceView
  private lateinit var overlay: TextView
  private var renderThread: RenderThread? = null
  private var surfaceWidth = 1
  private var surfaceHeight = 1
  private var refreshHz = 60f
  private lateinit var sensorManager: SensorManager
  private var rotationSensor: Sensor? = null
  private var displayRotation = Surface.ROTATION_0
  private var settings = AppSettings()

  private val sensorListener =
    object : SensorEventListener {
      private val quaternion = FloatArray(4)

      override fun onSensorChanged(event: SensorEvent) {
        if (event.sensor.type != Sensor.TYPE_ROTATION_VECTOR) return
        SensorManager.getQuaternionFromVector(quaternion, event.values)
        NativeSession.nativePushOrientation(
          quaternion[1],
          quaternion[2],
          quaternion[3],
          quaternion[0],
          event.timestamp,
          displayRotation,
        )
      }

      override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
    }

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    settings = SettingsStore.load(this)
    requestedOrientation = settings.screenOrientation.activityOrientation(vr = true)
    displayRotation = windowManager.defaultDisplay.rotation
    Log.i(TAG, "display rotation $displayRotation")
    window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    window.attributes =
      window.attributes.apply { screenBrightness = settings.brightness.coerceIn(0.05f, 1f) }
    if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O_MR1) {
      setShowWhenLocked(true)
      setTurnScreenOn(true)
    }
    WindowCompat.setDecorFitsSystemWindows(window, false)

    @Suppress("DEPRECATION")
    refreshHz = windowManager.defaultDisplay.refreshRate.takeIf { it > 1f } ?: 60f

    sensorManager = getSystemService(SensorManager::class.java)
    rotationSensor = sensorManager.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)

    surfaceView = SurfaceView(this)
    surfaceView.isClickable = true
    surfaceView.setOnClickListener { recenter() }
    surfaceView.holder.addCallback(this)
    overlay =
      TextView(this).apply {
        setTextColor(0xFFFFFFFF.toInt())
        textSize = 14f
        setPadding(32, 32, 32, 32)
        text = "Waiting for ALVR…"
        setOnClickListener { recenter() }
      }

    val root = FrameLayout(this)
    root.addView(
      surfaceView,
      FrameLayout.LayoutParams(
        FrameLayout.LayoutParams.MATCH_PARENT,
        FrameLayout.LayoutParams.MATCH_PARENT,
      ),
    )
    root.addView(
      overlay,
      FrameLayout.LayoutParams(
        FrameLayout.LayoutParams.WRAP_CONTENT,
        FrameLayout.LayoutParams.WRAP_CONTENT,
      ),
    )
    setContentView(root)
    hideSystemUi()

    lifecycleScope.launch {
      repeatOnLifecycle(Lifecycle.State.STARTED) {
        SessionRepository.state.collect { state ->
          overlay.visibility =
            if (state.isStreaming && !settings.debugOverlay) View.GONE else View.VISIBLE
          overlay.text = statusText(state)
          if (!state.isListening && !SessionRepository.isCreated()) {
            finish()
          }
        }
      }
    }
  }

  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    displayRotation = windowManager.defaultDisplay.rotation
    Log.i(TAG, "display rotation $displayRotation")
  }

  override fun onResume() {
    super.onResume()
    displayRotation = windowManager.defaultDisplay.rotation
    SessionRepository.startPolling()
    val sensor = rotationSensor
    if (sensor != null) {
      sensorManager.registerListener(
        sensorListener,
        sensor,
        SensorManager.SENSOR_DELAY_FASTEST,
      )
    }
  }

  override fun onPause() {
    sensorManager.unregisterListener(sensorListener)
    super.onPause()
  }

  override fun onDestroy() {
    sensorManager.unregisterListener(sensorListener)
    renderThread?.shutdown()
    renderThread?.join(1_000)
    renderThread = null
    super.onDestroy()
  }

  override fun surfaceCreated(holder: SurfaceHolder) {
    Log.i(TAG, "surfaceCreated")
    val frame = holder.surfaceFrame
    surfaceWidth = frame.width().coerceAtLeast(1)
    surfaceHeight = frame.height().coerceAtLeast(1)
    renderThread =
      RenderThread(holder.surface, surfaceWidth, surfaceHeight, refreshHz).also { it.start() }
  }

  override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
    Log.i(TAG, "surfaceChanged ${width}x${height}")
    surfaceWidth = width.coerceAtLeast(1)
    surfaceHeight = height.coerceAtLeast(1)
    NativeSession.nativeOnSurfaceChanged(surfaceWidth, surfaceHeight)
  }

  override fun surfaceDestroyed(holder: SurfaceHolder) {
    Log.i(TAG, "surfaceDestroyed")
    renderThread?.shutdown()
    renderThread?.join(1_000)
    renderThread = null
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus) hideSystemUi()
  }

  override fun onTouchEvent(event: MotionEvent): Boolean {
    if (event.actionMasked == MotionEvent.ACTION_UP) {
      recenter()
      return true
    }
    return super.onTouchEvent(event)
  }

  override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
    if (keyCode == KeyEvent.KEYCODE_VOLUME_DOWN) {
      recenter()
      return true
    }
    return super.onKeyDown(keyCode, event)
  }

  private fun recenter() {
    SessionRepository.recenter()
    overlay.text = "Recentered"
  }

  private fun hideSystemUi() {
    WindowInsetsControllerCompat(window, window.decorView).apply {
      hide(WindowInsetsCompat.Type.systemBars())
      systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
    }
  }

  private fun statusText(state: SessionUiState): String {
    if (!settings.debugOverlay) {
      return when (state.state) {
        "advertising" -> "Waiting for ALVR…\nStart ALVR on your PC and connect."
        "incoming_connection",
        "handshaking" -> "Server connected, handshaking…"
        "streaming" -> "Streaming"
        "reconnecting" -> "Reconnecting…"
        "error",
        "closed" -> "Session stopped"
        else -> state.label
      }
    }
    val stats = state.stats
    return buildString {
      appendLine("${state.label}  ${state.peer ?: ""}")
      state.stream?.let {
        appendLine("${it.width}x${it.height} @ ${"%.0f".format(it.refreshHz)} Hz")
      }
      appendLine("FPS %.1f  frame %.2f ms".format(stats.fps, stats.frameTimeMs))
      appendLine("latency %.1f ms  dropped %d".format(stats.latencyMs, stats.droppedFrames))
      appendLine("bitrate %.1f Mbps  ${stats.codec}".format(stats.bitrateMbps))
      appendLine("tracking %.0f Hz".format(stats.trackingHz))
    }
  }

  private class RenderThread(
    private val surface: Surface,
    private val width: Int,
    private val height: Int,
    private val refreshHz: Float,
  ) : Thread("PhoneALVR-Render") {
    @Volatile private var running = true

    override fun run() {
      Log.i(TAG, "render thread start ${width}x${height} @ ${refreshHz} Hz")
      if (!NativeSession.nativeOnSurfaceCreated(surface)) {
        Log.e(TAG, "nativeOnSurfaceCreated failed")
        return
      }
      Log.i(TAG, "nativeOnSurfaceCreated succeeded")
      NativeSession.nativeOnSurfaceChanged(width, height)
      // `nativeRenderFrame` blocks in eglSwapBuffers until the next vsync when
      // a new frame is presented. Do not add a second software pacer: waiting
      // for both the timer and the vsync halves the consumption rate and
      // backs up the decoder.
      while (running) {
        NativeSession.nativeRenderFrame()
      }
      NativeSession.nativeOnSurfaceDestroyed()
      Log.i(TAG, "render thread exit")
    }

    fun shutdown() {
      running = false
    }

    companion object {
      private const val TAG = "PhoneALVR-Render"
    }
  }
}
