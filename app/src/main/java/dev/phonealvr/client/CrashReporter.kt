package dev.phonealvr.client

import android.content.Context
import android.os.Build
import android.os.Process
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.time.Instant
import org.json.JSONObject

/**
 * Opt-in crash reporting. A report is always written to the app cache so the user can share it
 * manually. If the user enabled crash reporting and provided an HTTPS endpoint, a best-effort POST
 * is attempted before the process dies. No endpoint, token or identifier is hardcoded.
 */
object CrashReporter {
  fun install(context: Context) {
    val app = context.applicationContext
    val previous = Thread.getDefaultUncaughtExceptionHandler()
    Thread.setDefaultUncaughtExceptionHandler { thread, throwable ->
      runCatching { record(app, thread, throwable) }
      if (previous != null) {
        previous.uncaughtException(thread, throwable)
      } else {
        Process.killProcess(Process.myPid())
      }
    }
  }

  private fun record(app: Context, thread: Thread, throwable: Throwable) {
    val settings = SettingsStore.load(app)
    val report =
      JSONObject().apply {
        put("format", 1)
        put("app", "PhoneALVR")
        put("app_version", BuildConfig.VERSION_NAME)
        put("alvr_version", BuildConfig.ALVR_VERSION)
        put("timestamp", Instant.now().toString())
        put("thread", thread.name)
        put(
          "device",
          JSONObject().apply {
            put("manufacturer", Build.MANUFACTURER)
            put("model", Build.MODEL)
            put("android", Build.VERSION.RELEASE)
            put("sdk", Build.VERSION.SDK_INT)
          }
        )
        put("throwable", throwable.toString())
        put("stack", throwable.stackTraceToString())
        put(
          "native",
          if (NativeSession.load()) {
            runCatching { JSONObject(NativeSession.nativeGetDiagnostics()) }
              .getOrDefault(JSONObject())
          } else {
            JSONObject()
          },
        )
      }

    val directory = File(app.cacheDir, "diagnostics").apply { mkdirs() }
    val stamp = Instant.now().toString().replace(':', '-')
    File(directory, "crash-$stamp.json").writeText(report.toString(2))

    val endpoint = settings.crashReportingEndpoint.trim()
    if (settings.crashReporting && endpoint.startsWith("https://")) {
      runCatching { post(endpoint, report.toString()) }
    }
  }

  private fun post(endpoint: String, body: String) {
    val connection = URL(endpoint).openConnection() as HttpURLConnection
    try {
      connection.requestMethod = "POST"
      connection.connectTimeout = 3_000
      connection.readTimeout = 3_000
      connection.doOutput = true
      connection.setRequestProperty("Content-Type", "application/json")
      connection.outputStream.use { it.write(body.toByteArray()) }
      connection.responseCode
    } finally {
      connection.disconnect()
    }
  }
}
