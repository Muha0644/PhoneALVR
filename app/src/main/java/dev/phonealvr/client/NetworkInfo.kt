package dev.phonealvr.client

import android.os.Build
import java.net.Inet4Address
import java.net.NetworkInterface
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

data class NetworkInfo(
  val deviceName: String,
  val addresses: List<String>,
  val controlPort: Int = 9943,
  val streamPort: Int = 9944,
)

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
      val deviceName = "${Build.MANUFACTURER} ${Build.MODEL}".trim()
      NetworkInfo(deviceName = deviceName, addresses = addresses.distinct())
    }
}
