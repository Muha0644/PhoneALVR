package dev.phonealvr.client

import android.os.Build
import java.net.Inet4Address
import java.net.InetAddress
import java.net.NetworkInterface
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

data class NetworkInfo(
  val hostname: String,
  val addresses: List<String>,
  val controlPort: Int = 9943,
  val streamPort: Int = 9944,
) {
  fun describe(): String = buildString {
    appendLine("PhoneALVR waiting")
    appendLine("Hostname: $hostname")
    appendLine("LAN IPs: ${addresses.joinToString().ifEmpty { "none" }}")
    appendLine("Control port: $controlPort")
    appendLine("Stream port: $streamPort")
    appendLine("In ALVR: Connection → Add device → enter one of the LAN IPs.")
  }
}

object NetworkInfoProvider {
  suspend fun load(): NetworkInfo =
    withContext(Dispatchers.IO) {
      val addresses = mutableListOf<String>()
      runCatching {
        NetworkInterface.getNetworkInterfaces().toList().forEach { network ->
          if (!network.isUp || network.isLoopback) return@forEach
          network.inetAddresses.toList().forEach { address ->
            if (address is Inet4Address && !address.isLoopbackAddress) {
              address.hostAddress?.let(addresses::add)
            }
          }
        }
      }
      val hostname = runCatching { InetAddress.getLocalHost().hostName }.getOrNull() ?: Build.MODEL
      NetworkInfo(hostname = hostname, addresses = addresses.distinct())
    }
}
