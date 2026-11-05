package dev.phonealvr.client.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material.icons.filled.Videocam
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.phonealvr.client.AppSettings
import dev.phonealvr.client.NetworkInfo
import dev.phonealvr.client.SessionUiState
import kotlin.math.max
import kotlin.math.min

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
  state: SessionUiState,
  settings: AppSettings,
  network: NetworkInfo,
  onStart: () -> Unit,
  onStop: () -> Unit,
  onOpenVr: () -> Unit,
  onRecenter: () -> Unit,
  onScan: () -> Unit,
  onExport: () -> Unit,
  onSaveSettings: (AppSettings) -> Unit,
) {
  var showSettings by remember { mutableStateOf(false) }
  val clipboard = LocalClipboardManager.current
  val displayMetrics = LocalContext.current.resources.displayMetrics
  val nativePerEyeWidth = max(displayMetrics.widthPixels, displayMetrics.heightPixels) / 2
  val nativePerEyeHeight = min(displayMetrics.widthPixels, displayMetrics.heightPixels)

  Scaffold(
    topBar = {
      TopAppBar(
        title = { Text("PhoneALVR") },
        actions = {
          OutlinedButton(onClick = onExport) {
            Icon(Icons.Default.Share, contentDescription = null)
            Spacer(Modifier.width(8.dp))
            Text("Diagnostics")
          }
        },
      )
    },
  ) { padding ->
    LazyColumn(
      modifier = Modifier.fillMaxSize().padding(padding).padding(16.dp),
      verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
      item { StatusCard(state) }

      item {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
          if (state.isListening) {
            Button(onClick = onStop) {
              Icon(Icons.Default.Stop, contentDescription = null)
              Spacer(Modifier.width(8.dp))
              Text("Stop waiting")
            }
            OutlinedButton(onClick = onOpenVr, enabled = state.isStreaming) {
              Icon(Icons.Default.Videocam, contentDescription = null)
              Spacer(Modifier.width(8.dp))
              Text("Open VR")
            }
          } else {
            Button(onClick = onStart) {
              Icon(Icons.Default.PlayArrow, contentDescription = null)
              Spacer(Modifier.width(8.dp))
              Text("Start waiting")
            }
          }
        }
      }

      if (state.isStreaming) {
        item {
          Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onRecenter) { Text("Recenter") }
            OutlinedButton(onClick = onScan) {
              Icon(Icons.Default.QrCodeScanner, contentDescription = null)
              Spacer(Modifier.width(8.dp))
              Text("Viewer QR")
            }
            OutlinedButton(onClick = { showSettings = true }) {
              Icon(Icons.Default.Settings, contentDescription = null)
              Spacer(Modifier.width(8.dp))
              Text("Settings")
            }
          }
        }
      } else {
        item {
          Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onScan) {
              Icon(Icons.Default.QrCodeScanner, contentDescription = null)
              Spacer(Modifier.width(8.dp))
              Text("Scan viewer")
            }
            OutlinedButton(onClick = { showSettings = true }) {
              Icon(Icons.Default.Settings, contentDescription = null)
              Spacer(Modifier.width(8.dp))
              Text("Settings")
            }
          }
        }
      }

      state.hudMessage?.let { message ->
        item {
          Card(
            colors =
              CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant)
          ) {
            Text(message, Modifier.padding(12.dp), style = MaterialTheme.typography.bodySmall)
          }
        }
      }

      state.error?.let { error ->
        item {
          Card(
            colors =
              CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer),
          ) {
            Column(Modifier.padding(16.dp)) {
              Text("Error", style = MaterialTheme.typography.titleMedium)
              Text(error)
            }
          }
        }
      }

      item {
        WaitingCard(
          network,
          state,
          onCopy = { clipboard.setText(AnnotatedString(network.describe())) }
        )
      }

      if (state.state == "idle" || state.state == "closed") {
        item { OnboardingCard() }
      }

      item {
        Card {
          Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Session settings", style = MaterialTheme.typography.titleMedium)
            Text(
              "Viewer: ${settings.viewerProfile?.let { "custom profile" } ?: "built-in Cardboard v1"}"
            )
            Text("Brightness: ${"%.0f".format(settings.brightness * 100)}%")
            Text("Refresh: ${settings.refreshRateHz?.let { "%.0f Hz".format(it) } ?: "automatic"}")
            Text(
              "Resolution cap: " +
                if (settings.resolutionCapWidth > 0) {
                  "${settings.resolutionCapWidth}x${settings.resolutionCapHeight} per eye"
                } else {
                  "native panel"
                },
            )
            Text("Native per-eye resolution: ${nativePerEyeWidth}x${nativePerEyeHeight}")
            state.stream?.let { stream ->
              if (stream.width > nativePerEyeWidth || stream.height > nativePerEyeHeight) {
                Text(
                  "Warning: the stream is larger than the panel. Set ALVR resolution to Scale 1.0.",
                  color = MaterialTheme.colorScheme.error,
                  style = MaterialTheme.typography.bodySmall,
                )
              }
            }
          }
        }
      }
    }
  }

  if (showSettings) {
    SettingsDialog(
      initial = settings,
      onDismiss = { showSettings = false },
      onSave = {
        showSettings = false
        onSaveSettings(it)
      },
      onScan = {
        showSettings = false
        onScan()
      },
    )
  }
}

@Composable
private fun StatusCard(state: SessionUiState) {
  val accent =
    when (state.state) {
      "streaming" -> Color(0xFF69F0AE)
      "advertising",
      "incoming_connection",
      "handshaking" -> Color(0xFFFFD180)
      "reconnecting" -> Color(0xFFFFAB40)
      "error",
      "closed" -> Color(0xFFFF5252)
      else -> Color(0xFF90A4AE)
    }

  Card {
    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
      Row(verticalAlignment = Alignment.CenterVertically) {
        if (state.state == "incoming_connection" || state.state == "handshaking") {
          CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
        } else {
          Text("●", color = accent, style = MaterialTheme.typography.titleLarge)
        }
        Spacer(Modifier.width(10.dp))
        Text(state.label.replace('_', ' '), style = MaterialTheme.typography.titleLarge)
      }
      state.peer?.let { Text("Streamer: $it", fontFamily = FontFamily.Monospace) }
      state.stream?.let {
        Text("Stream: ${it.width}×${it.height} per eye @ ${"%.0f".format(it.refreshHz)} Hz")
      }
      if (state.isStreaming) {
        HorizontalDivider()
        Text(
          "FPS %.1f · frame %.2f ms · latency %.1f ms · dropped %d"
            .format(
              state.stats.fps,
              state.stats.frameTimeMs,
              state.stats.latencyMs,
              state.stats.droppedFrames,
            ),
          style = MaterialTheme.typography.bodySmall,
        )
        Text(
          "Bitrate %.1f Mbps · tracking %.0f Hz · codec %s"
            .format(
              state.stats.bitrateMbps,
              state.stats.trackingHz,
              state.stats.codec,
            ),
          style = MaterialTheme.typography.bodySmall,
        )
      }
    }
  }
}

@Composable
private fun WaitingCard(network: NetworkInfo, state: SessionUiState, onCopy: () -> Unit) {
  Card {
    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
      Row(verticalAlignment = Alignment.CenterVertically) {
        Text("Connection", style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.width(12.dp))
        AssistChip(onClick = onCopy, label = { Text("Copy addresses") })
      }
      Text("Hostname: ${network.hostname}", fontFamily = FontFamily.Monospace)
      Text("LAN IPs: ${network.addresses.ifEmpty { listOf("none detected") }.joinToString()}")
      Text(
        "Listening ports: ${network.controlPort} (control), ${network.streamPort} (stream)",
        fontFamily = FontFamily.Monospace,
      )
      Spacer(Modifier.height(4.dp))
      Text(
        if (state.isListening) {
          "On the PC, open ALVR. This phone should appear in the dashboard. " +
            "If it does not, add it by IP in ALVR → Connection → Add device."
        } else {
          "Press Start waiting, then open ALVR on the PC."
        },
        style = MaterialTheme.typography.bodySmall,
      )
      Text(
        "PhoneALVR never dials the PC: ALVR's streamer connects to this phone.",
        style = MaterialTheme.typography.bodySmall,
      )
    }
  }
}

@Composable
private fun OnboardingCard() {
  Card(
    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant)
  ) {
    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
      Text("First time?", style = MaterialTheme.typography.titleMedium)
      Text(
        "1. Install ALVR on your PC and start it.\n" +
          "2. Put the phone in the same Wi-Fi network (or connect USB with ADB).\n" +
          "3. Press Start waiting. ALVR connects to the phone; the phone never dials the PC.\n" +
          "4. In ALVR, trust the new device and choose your stream settings.\n" +
          "5. Put the phone in a Cardboard viewer and press Open VR when streaming starts.",
      )
      Text(
        "Wired mode: set ALVR → Connection → Wired Client Type → Custom to " +
          "dev.phonealvr.client (the debug build uses dev.phonealvr.client.debug).",
        style = MaterialTheme.typography.bodySmall,
      )
      Text(
        "For long sessions, set Android Settings → Apps → PhoneALVR → Battery to " +
          "Unrestricted so the vendor power manager does not kill the listener.",
        style = MaterialTheme.typography.bodySmall,
      )
    }
  }
}

@Composable
private fun SettingsDialog(
  initial: AppSettings,
  onDismiss: () -> Unit,
  onSave: (AppSettings) -> Unit,
  onScan: () -> Unit,
) {
  var draft by remember(initial) { mutableStateOf(initial) }
  var capWidth by
    remember(initial) {
      mutableStateOf(initial.resolutionCapWidth.takeIf { it > 0 }?.toString() ?: "")
    }
  var capHeight by
    remember(initial) {
      mutableStateOf(initial.resolutionCapHeight.takeIf { it > 0 }?.toString() ?: "")
    }
  var manualIpd by remember { mutableStateOf("64") }
  var manualScreenToLens by remember { mutableStateOf("42") }
  var manualFov by remember { mutableStateOf("90") }
  var manualK1 by remember { mutableStateOf("0.441") }
  var manualK2 by remember { mutableStateOf("0.156") }

  AlertDialog(
    onDismissRequest = onDismiss,
    title = { Text("Settings") },
    text = {
      LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        item {
          Text("Brightness ${"%.0f".format(draft.brightness * 100)}%")
          Slider(
            value = draft.brightness,
            onValueChange = { draft = draft.copy(brightness = it) },
            valueRange = 0.05f..1f,
          )
        }
        item {
          Text("Vignette ${"%.0f".format(draft.vignette * 100)}%")
          Slider(
            value = draft.vignette,
            onValueChange = { draft = draft.copy(vignette = it) },
            valueRange = 0f..1f,
          )
        }
        item {
          Text("Chromatic aberration ${"%.1f".format(draft.chromaticAberration * 1000)} mm")
          Slider(
            value = draft.chromaticAberration,
            onValueChange = { draft = draft.copy(chromaticAberration = it) },
            valueRange = 0f..0.05f,
          )
        }
        item {
          Text(
            "Refresh rate: " + (draft.refreshRateHz?.let { "%.0f Hz".format(it) } ?: "automatic"),
          )
          Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            listOf(null, 60f, 72f, 90f, 120f).forEach { rate ->
              AssistChip(
                onClick = { draft = draft.copy(refreshRateHz = rate) },
                label = { Text(rate?.let { "%.0f".format(it) } ?: "Auto") },
              )
            }
          }
        }
        item {
          Text(
            "Resolution cap per eye (blank = native panel). Resolution and refresh changes apply on the next session start.",
            style = MaterialTheme.typography.bodySmall,
          )
          Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
              value = capWidth,
              onValueChange = { capWidth = it.filter(Char::isDigit) },
              label = { Text("width") },
              keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
              singleLine = true,
              modifier = Modifier.width(120.dp),
            )
            OutlinedTextField(
              value = capHeight,
              onValueChange = { capHeight = it.filter(Char::isDigit) },
              label = { Text("height") },
              keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
              singleLine = true,
              modifier = Modifier.width(120.dp),
            )
          }
        }
        item {
          SwitchRow("Debug HUD", draft.debugOverlay) { draft = draft.copy(debugOverlay = it) }
        }
        item {
          SwitchRow("Prefer wired setup", draft.preferWired) {
            draft = draft.copy(preferWired = it)
          }
        }
        item {
          SwitchRow("Opt-in crash reporting", draft.crashReporting) {
            draft = draft.copy(crashReporting = it)
          }
        }
        if (draft.crashReporting) {
          item {
            OutlinedTextField(
              value = draft.crashReportingEndpoint,
              onValueChange = { draft = draft.copy(crashReportingEndpoint = it) },
              label = { Text("Crash endpoint (https://…)") },
              singleLine = true,
              modifier = Modifier.fillMaxWidth(),
            )
          }
        }
        item {
          HorizontalDivider()
          Text("Viewer profile")
          Text(
            draft.viewerProfile?.let { "Custom profile active" } ?: "Built-in Cardboard v1",
            style = MaterialTheme.typography.bodySmall,
          )
          Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onScan) { Text("Scan QR") }
            if (draft.viewerProfile != null) {
              TextButton(onClick = { draft = draft.copy(viewerProfile = null) }) {
                Text("Use built-in")
              }
            }
          }
        }
        item {
          Text("Manual viewer (millimetres / degrees)", style = MaterialTheme.typography.titleSmall)
          Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
              value = manualIpd,
              onValueChange = { manualIpd = it.filter { c -> c.isDigit() || c == '.' } },
              label = { Text("IPD mm") },
              singleLine = true,
              modifier = Modifier.width(100.dp),
            )
            OutlinedTextField(
              value = manualScreenToLens,
              onValueChange = { manualScreenToLens = it.filter { c -> c.isDigit() || c == '.' } },
              label = { Text("screen-lens") },
              singleLine = true,
              modifier = Modifier.width(120.dp),
            )
            OutlinedTextField(
              value = manualFov,
              onValueChange = { manualFov = it.filter { c -> c.isDigit() || c == '.' } },
              label = { Text("FOV°") },
              singleLine = true,
              modifier = Modifier.width(90.dp),
            )
          }
          Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
              value = manualK1,
              onValueChange = { manualK1 = it.filter { c -> c.isDigit() || c == '.' || c == '-' } },
              label = { Text("k1") },
              singleLine = true,
              modifier = Modifier.width(100.dp),
            )
            OutlinedTextField(
              value = manualK2,
              onValueChange = { manualK2 = it.filter { c -> c.isDigit() || c == '.' || c == '-' } },
              label = { Text("k2") },
              singleLine = true,
              modifier = Modifier.width(100.dp),
            )
          }
          OutlinedButton(
            onClick = {
              val ipd = manualIpd.toFloatOrNull()
              val screen = manualScreenToLens.toFloatOrNull()
              val fov = manualFov.toFloatOrNull()
              if (
                ipd != null &&
                  ipd in 20f..150f &&
                  screen != null &&
                  screen in 5f..250f &&
                  fov != null &&
                  fov in 20f..120f
              ) {
                draft =
                  draft.copy(
                    viewerProfile =
                      """{"vendor":"Manual","model":"Custom viewer","interLensDistance":${ipd / 1000f},"screenToLensDistance":${screen / 1000f},"trayToLensDistance":0.035,"distortionCoefficients":[0.441,0.156,0,0,0,0,0,0],"fov":$fov}""",
                  )
              }
            },
          ) {
            Text("Apply manual profile")
          }
        }
      }
    },
    confirmButton = {
      Button(
        onClick = {
          val width = capWidth.toIntOrNull() ?: 0
          val height = capHeight.toIntOrNull() ?: 0
          val complete = width > 0 && height > 0
          onSave(
            draft.copy(
              resolutionCapWidth = if (complete) width else 0,
              resolutionCapHeight = if (complete) height else 0,
            ),
          )
        },
      ) {
        Text("Save")
      }
    },
    dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
  )
}

@Composable
private fun SwitchRow(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
  Row(
    modifier = Modifier.fillMaxWidth(),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.SpaceBetween,
  ) {
    Text(label)
    Switch(checked = checked, onCheckedChange = onCheckedChange)
  }
}
