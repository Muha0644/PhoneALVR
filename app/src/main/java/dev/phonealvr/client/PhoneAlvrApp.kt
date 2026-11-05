package dev.phonealvr.client

import android.app.Application

class PhoneAlvrApp : Application() {
  override fun onCreate() {
    super.onCreate()
    CrashReporter.install(this)
  }
}
