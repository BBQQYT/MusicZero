package com.musiczero.app

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

/** The five most recent tracks; tapping one replays it through the engine. */
@Composable
fun HistoryScreen(
    state: PlayerState,
    onOpenPlayer: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .statusBarsPadding(),
    ) {
        Text(
            text = stringResource(R.string.tab_history),
            style = MaterialTheme.typography.headlineSmall,
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 14.dp),
        )
        val entries = state.history
        if (entries.isEmpty()) {
            EmptyState(
                glyph = MzGlyph.Clock,
                text = stringResource(R.string.empty_history),
            )
        } else {
            LazyColumn(contentPadding = PaddingValues(bottom = 18.dp)) {
                items(entries.size) { index ->
                    val track = entries[index]
                    TrackRow(
                        track = track,
                        seed = track.id,
                        onClick = {
                            state.replay(track)
                            onOpenPlayer()
                        },
                        trailing = {
                            IconButton(
                                onClick = { state.replay(track) },
                                modifier = Modifier.size(42.dp),
                            ) {
                                MzIcon(
                                    glyph = MzGlyph.Replay,
                                    size = 21.dp,
                                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        },
                    )
                }
            }
        }
    }
}
