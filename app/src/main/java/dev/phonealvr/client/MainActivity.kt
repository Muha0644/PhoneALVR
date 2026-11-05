package dev.phonealvr.client

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import dev.phonealvr.client.ui.HomeScreen
import dev.phonealvr.client.ui.PhoneAlvrTheme
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
  private var settings by mutableStateOf(AppSettings())
  private var network by mutableStateOf(NetworkInfo(Build.MODEL, emptyList()))

  private val notificationPermission =
    registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

  private val cameraPermission =
    registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
      if (granted) {
        launchScanner()
      } else {
        toast("Camera permission is only needed to scan viewer QR codes")
      }
    }

  private val scanLauncher =
    registerForActivityResult(ScanContract()) { result ->
      val contents = result.contents
      if (contents.isNullOrBlank()) return@registerForActivityResult
      if (SettingsStore.applyViewerProfile(this, contents)) {
        settings = SettingsStore.load(this)
        toast("Viewer profile applied")
      } else {
        toast("That QR code is not a valid Cardboard viewer profile")
      }
    }

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    settings = SettingsStore.load(this)
    requestNotificationPermission()

    setContent {
      PhoneAlvrTheme {
        val state by SessionRepository.state.collectAsStateWithLifecycle()
        HomeScreen(
          state = state,
          settings = settings,
          network = network,
          onStart = ::startSession,
          onStop = ::stopSession,
          onOpenVr = ::openVr,
          onRecenter = SessionRepository::recenter,
          onScan = ::scanViewer,
          onExport = { DiagnosticsExporter.share(this) },
          onSaveSettings = ::saveSettings,
        )
      }
    }

    lifecycleScope.launch { network = NetworkInfoProvider.load() }
  }

  override fun onResume() {
    super.onResume()
    settings = SettingsStore.load(this)
    SessionRepository.startPolling()
    lifecycleScope.launch { network = NetworkInfoProvider.load() }
  }

  private fun startSession() {
    requestNotificationPermission()
    if (!SessionRepository.start(this)) {
      toast("Could not start the native session; check logcat tag PhoneALVR")
      return
    }
    ContextCompat.startForegroundService(this, SessionService.startIntent(this))
    startActivity(Intent(this, VrActivity::class.java))
  }

  private fun stopSession() {
    startService(SessionService.stopIntent(this))
  }

  private fun openVr() {
    if (!SessionRepository.isCreated()) {
      toast("Start waiting first")
      return
    }
    startActivity(Intent(this, VrActivity::class.java))
  }

  private fun scanViewer() {
    val granted =
      ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) ==
        PackageManager.PERMISSION_GRANTED
    if (granted) {
      launchScanner()
    } else {
      cameraPermission.launch(Manifest.permission.CAMERA)
    }
  }

  private fun launchScanner() {
    val options =
      ScanOptions()
        .setPrompt("Scan the Cardboard viewer QR code")
        .setBeepEnabled(false)
        .setOrientationLocked(false)
    scanLauncher.launch(options)
  }

  private fun saveSettings(updated: AppSettings) {
    val applied = SettingsStore.saveAndApply(this, updated)
    settings = SettingsStore.load(this)
    toast(
      when {
        applied -> "Settings applied"
        SessionRepository.isCreated() -> "Saved, but the native session rejected the update"
        else -> "Saved; changes apply on the next session start"
      },
    )
  }

  private fun requestNotificationPermission() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      val granted =
        ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) ==
          PackageManager.PERMISSION_GRANTED
      if (!granted) {
        notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
      }
    }
  }

  private fun toast(message: String) {
    Toast.makeText(this, message, Toast.LENGTH_SHORT).show()
  }
}
