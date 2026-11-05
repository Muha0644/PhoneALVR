package dev.phonealvr.client

import android.content.Context
import android.content.ContextWrapper
import android.content.pm.PackageManager

/**
 * ALVR's client core unconditionally asks for RECORD_AUDIO in `ClientCoreContext::new`. PhoneALVR
 * does not implement microphone passthrough and therefore does not declare that permission.
 * Returning "granted" here prevents an invalid permission request from an application context;
 * audio capture (if a streamer enables it) fails gracefully in the audio backend instead of
 * aborting the session.
 */
internal class AlvrContext(base: Context) : ContextWrapper(base) {
  override fun checkSelfPermission(permission: String): Int = PackageManager.PERMISSION_GRANTED
}
