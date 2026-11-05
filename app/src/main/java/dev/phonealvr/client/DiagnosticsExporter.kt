package dev.phonealvr.client

import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.content.FileProvider
import java.io.File
import java.time.Instant
import org.json.JSONObject

object DiagnosticsExporter {
  fun share(context: Context) {
    val report = buildReport(context)
    val directory = File(context.cacheDir, "diagnostics").apply { mkdirs() }
    val stamp = Instant.now().toString().replace(':', '-')
    val file = File(directory, "phonealvr-$stamp.json")
    file.writeText(report)
    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
    val intent =
      Intent(Intent.ACTION_SEND).apply {
        type = "application/json"
        putExtra(Intent.EXTRA_STREAM, uri)
        putExtra(Intent.EXTRA_SUBJECT, "PhoneALVR diagnostics")
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
    context.startActivity(Intent.createChooser(intent, "Share diagnostics"))
  }

  fun buildReport(context: Context): String {
    val settings = SettingsStore.load(context).copy(crashReportingEndpoint = "")
    val native =
      if (NativeSession.load()) {
        runCatching { NativeSession.nativeGetDiagnostics() }.getOrDefault("{}")
      } else {
        "{}"
      }
    val root = JSONObject()
    root.put("format", 1)
    root.put("app", "PhoneALVR")
    root.put("app_version", BuildConfig.VERSION_NAME)
    root.put("alvr_version", BuildConfig.ALVR_VERSION)
    root.put("timestamp", Instant.now().toString())
    root.put(
      "device",
      JSONObject().apply {
        put("manufacturer", Build.MANUFACTURER)
        put("model", Build.MODEL)
        put("device", Build.DEVICE)
        put("android", Build.VERSION.RELEASE)
        put("sdk", Build.VERSION.SDK_INT)
        put("abis", Build.SUPPORTED_ABIS.joinToString())
      }
    )
    root.put("settings", JSONObject(settings.toNativeJson()))
    root.put("native", runCatching { JSONObject(native) }.getOrDefault(JSONObject()))
    root.put("note", "Crash-reporting endpoints and device identifiers are not included.")
    return root.toString(2)
  }
}
