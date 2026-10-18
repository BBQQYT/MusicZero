package com.musiczero.app

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import java.util.Calendar

/**
 * Retro-style home: a greeting header, provider cards, the recent-track
 * carousel and the playlist rail for the active provider.
 */
@Composable
fun HomeScreen(
    state: PlayerState,
    onOpenPlayer: () -> Unit,
    modifier: Modifier = Modifier,
) {
    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .statusBarsPadding(),
        contentPadding = PaddingValues(bottom = 18.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item { HomeHeader(state) }
        if (state.modules.isNotEmpty()) {
            item { SectionTitle(stringResource(R.string.section_providers)) }
            item {
                LazyRow(
                    contentPadding = PaddingValues(horizontal = 20.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    items(state.modules.size) { index ->
                        val module = state.modules[index]
                        ModuleCard(
                            module = module,
                            selected = state.status?.module == module.id,
                            onClick = { state.switchTo(module.id) },
                        )
                    }
                }
            }
        }
        val history = state.history
        if (history.isNotEmpty()) {
            item { SectionTitle(stringResource(R.string.section_recent)) }
            item {
                LazyRow(
                    contentPadding = PaddingValues(horizontal = 20.dp),
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    items(history.size) { index ->
                        val track = history[index]
                        HistoryCard(track = track) {
                            state.replay(track)
                            onOpenPlayer()
                        }
                    }
                }
            }
        }
        item { SectionTitle(stringResource(R.string.section_playlists)) }
        item {
            val entries = state.playlists.playlists
            if (entries.isEmpty()) {
                EmptyState(
                    glyph = MzGlyph.Note,
                    text = stringResource(R.string.empty_playlists),
                )
            } else {
                LazyRow(
                    contentPadding = PaddingValues(horizontal = 20.dp),
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
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
                            modifier = Modifier.width(150.dp),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun HomeHeader(state: PlayerState) {
    val subtitle = when {
        state.modules.isEmpty() -> stringResource(R.string.no_modules)
        state.connection == Connection.Starting -> stringResource(R.string.engine_starting)
        state.status != null -> state.status!!.name
        else -> stringResource(R.string.engine_down)
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp)
            .padding(top = 16.dp, bottom = 4.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(
            text = greetingText(),
            style = MaterialTheme.typography.displaySmall,
        )
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            StatusDot(state.connection)
            Text(
                text = "MusicZero · $subtitle",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun greetingText(): String {
    val hour = remember { Calendar.getInstance().get(Calendar.HOUR_OF_DAY) }
    val greeting = when (hour) {
        in 5..11 -> R.string.greeting_morning
        in 12..16 -> R.string.greeting_day
        in 17..22 -> R.string.greeting_evening
        else -> R.string.greeting_night
    }
    return stringResource(greeting)
}

@Composable
private fun HistoryCard(track: Track, onClick: () -> Unit) {
    Column(
        modifier = Modifier
            .width(138.dp)
            .clickable(onClick = onClick),
    ) {
        ArtImage(
            url = track.artUrl,
            seed = track.id,
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(1f)
                .clip(RoundedCornerShape(20.dp)),
            glyphSize = 44.dp,
        )
        Spacer(Modifier.width(2.dp))
        Text(
            text = track.title.ifEmpty { "—" },
            style = MaterialTheme.typography.bodyMedium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(top = 8.dp),
        )
        Text(
            text = track.artist,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
