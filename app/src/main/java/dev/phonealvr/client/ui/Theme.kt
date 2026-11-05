package dev.phonealvr.client.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val PhoneAlvrColors =
  darkColorScheme(
    primary = Color(0xFF6FD3FF),
    onPrimary = Color(0xFF003344),
    secondary = Color(0xFF8BC34A),
    background = Color(0xFF101418),
    surface = Color(0xFF182028),
    surfaceVariant = Color(0xFF24303A),
    error = Color(0xFFFF5252),
  )

@Composable
fun PhoneAlvrTheme(content: @Composable () -> Unit) {
  MaterialTheme(colorScheme = PhoneAlvrColors, content = content)
}
