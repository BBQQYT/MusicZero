package com.musiczero.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

/**
 * The playlist library: a provider filter row on top of a two-column grid of
 * gradient tiles. Tapping a tile asks the engine to play that playlist.
 */
@Composable
fun LibraryScreen(
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
            text = stringResource(R.string.tab_playlists),
            style = MaterialTheme.typography.headlineSmall,
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 14.dp),
        )
        if (state.modules.size > 1) {
            LazyRow(
                contentPadding = PaddingValues(horizontal = 20.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(state.modules.size) { index ->
                    val module = state.modules[index]
                    PillChip(
                        text = module.name,
                        selected = state.status?.module == module.id,
                        onClick = { state.switchTo(module.id) },
                    )
                }
            }
        }
        val entries = state.playlists.playlists
        if (entries.isEmpty()) {
            EmptyState(
                glyph = MzGlyph.Note,
                text = stringResource(R.string.empty_playlists),
            )
        } else {
            LazyVerticalGrid(
                columns = GridCells.Fixed(2),
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(20.dp, 14.dp, 20.dp, 20.dp),
                horizontalArrangement = Arrangement.spacedBy(14.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                items(entries.size) { index ->
                    val playlist = entries[index]
                    PlaylistCard(
                        playlist = playlist,
                        selected = playlist.id == state.playlists.playlist,
                        playing = playlist.id == state.status?.playlist && state.status?.status == "Playing",
                        onClick = {
                            state.pickPlaylist(playlist.id)
                            onOpenPlayer()
                        },
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        }
    }
}
