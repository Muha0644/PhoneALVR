package dev.phonealvr.client

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import dev.phonealvr.client.ui.HomeScreen
import dev.phonealvr.client.ui.PhoneAlvrTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class HomeScreenTest {
  @get:Rule val composeRule = createComposeRule()

  private fun render(state: SessionUiState, onStart: () -> Unit = {}) {
    composeRule.setContent {
      PhoneAlvrTheme {
        HomeScreen(
          state = state,
          settings = AppSettings(),
          network = NetworkInfo("test-host", listOf("192.0.2.5")),
          onStart = onStart,
          onStop = {},
          onOpenVr = {},
          onRecenter = {},
          onScan = {},
          onExport = {},
          onSaveSettings = {},
        )
      }
    }
  }

  @Test
  fun idleScreenOffersStart() {
    var started = false
    render(SessionUiState()) { started = true }
    composeRule.onNodeWithText("Start waiting").assertIsDisplayed().performClick()
    assertTrue(started)
  }

  @Test
  fun streamingScreenShowsStreamDetails() {
    render(
      SessionUiState(
        state = "streaming",
        label = "streaming",
        stream = StreamUi(1184, 1056, 90f, 20),
      ),
    )
    composeRule.onNodeWithText("Stream: 1184×1056 per eye @ 90 Hz").assertIsDisplayed()
    composeRule.onNodeWithText("Open VR").assertIsDisplayed()
  }
}
