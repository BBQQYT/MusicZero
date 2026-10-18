@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package com.musiczero.app

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/** Bottom navigation destinations. */
enum class Tab(val glyph: MzGlyph, val label: Int) {
    Home(MzGlyph.Home, R.string.tab_home),
    Playlists(MzGlyph.Grid, R.string.tab_playlists),
    History(MzGlyph.Clock, R.string.tab_history),
    Settings(MzGlyph.Tune, R.string.tab_settings),
}

/**
 * The whole app: bottom navigation, the mini player parked above it and the
 * full-screen player sliding up over everything. All state flows from the
 * engine socket through [PlayerState].
 */
@Composable
fun MusicZeroApp() {
    val context = LocalContext.current
    val state = remember { PlayerState(context) }
    LaunchedEffect(state) { state.start() }

    var tab by remember { mutableStateOf(Tab.Home) }
    var playerOpen by remember { mutableStateOf(false) }
    var playlistsSheet by remember { mutableStateOf(false) }
    var historySheet by remember { mutableStateOf(false) }
    val playlistsSheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val historySheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

    BackHandler(enabled = playerOpen) { playerOpen = false }

    Box(Modifier.fillMaxSize()) {
        Scaffold(
            containerColor = MaterialTheme.colorScheme.background,
            contentWindowInsets = WindowInsets(0, 0, 0, 0),
            bottomBar = {
                Column {
                    val status = state.status
                    AnimatedVisibility(
                        visible = status != null && !playerOpen,
                        enter = slideInVertically(initialOffsetY = { it }) + fadeIn(tween(180)),
                        exit = slideOutVertically(targetOffsetY = { it }) + fadeOut(tween(140)),
                    ) {
                        val current = status
                        if (current != null) {
                            MiniPlayer(
                                status = current,
                                onOpen = { playerOpen = true },
                                onToggle = { state.toggle() },
                                onNext = { state.next() },
                            )
                        }
                    }
                    NavigationBar(containerColor = MaterialTheme.colorScheme.surface) {
                        Tab.entries.forEach { entry ->
                            NavigationBarItem(
                                selected = tab == entry,
                                onClick = { tab = entry },
                                icon = { MzIcon(entry.glyph, size = 24.dp) },
                                label = { Text(stringResource(entry.label)) },
                                colors = NavigationBarItemDefaults.colors(
                                    selectedIconColor = MaterialTheme.colorScheme.primary,
                                    selectedTextColor = MaterialTheme.colorScheme.primary,
                                    indicatorColor = MaterialTheme.colorScheme.primary.copy(alpha = 0.14f),
                                    unselectedIconColor = MaterialTheme.colorScheme.onSurfaceVariant,
                                    unselectedTextColor = MaterialTheme.colorScheme.onSurfaceVariant,
                                ),
                            )
                        }
                    }
                }
            },
        ) { padding ->
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding),
            ) {
                Crossfade(targetState = tab, animationSpec = tween(200)) { current ->
                    when (current) {
                        Tab.Home -> HomeScreen(state, onOpenPlayer = { playerOpen = true })
                        Tab.Playlists -> LibraryScreen(state, onOpenPlayer = { playerOpen = true })
                        Tab.History -> HistoryScreen(state, onOpenPlayer = { playerOpen = true })
                        Tab.Settings -> SettingsScreen(state)
                    }
                }
            }
        }

        AnimatedVisibility(
            visible = playerOpen,
            enter = slideInVertically(initialOffsetY = { it }) + fadeIn(tween(220)),
            exit = slideOutVertically(targetOffsetY = { it }) + fadeOut(tween(180)),
            modifier = Modifier.fillMaxSize(),
        ) {
            NowPlayingScreen(
                state = state,
                onClose = { playerOpen = false },
                onOpenPlaylists = { playlistsSheet = true },
                onOpenHistory = { historySheet = true },
                onOpenLibrary = {
                    playerOpen = false
                    tab = Tab.Playlists
                },
            )
        }
    }

    if (playlistsSheet) {
        ModalBottomSheet(
            onDismissRequest = { playlistsSheet = false },
            sheetState = playlistsSheetState,
        ) {
            PlaylistsSheet(state) { playlistsSheet = false }
        }
    }
    if (historySheet) {
        ModalBottomSheet(
            onDismissRequest = { historySheet = false },
            sheetState = historySheetState,
        ) {
            HistorySheet(state) { historySheet = false }
        }
    }
}

@Composable
private fun PlaylistsSheet(state: PlayerState, onClose: () -> Unit) {
    Column(
        modifier = Modifier
            .navigationBarsPadding()
            .padding(bottom = 10.dp),
    ) {
        Text(
            text = stringResource(R.string.sheet_playlists),
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier.padding(horizontal = 22.dp, vertical = 8.dp),
        )
        val entries = state.playlists.playlists
        if (entries.isEmpty()) {
            Text(
                text = stringResource(R.string.empty_playlists),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(22.dp),
            )
        } else {
            LazyColumn {
                items(entries.size) { index ->
                    val playlist = entries[index]
                    val selected = playlist.id == state.playlists.playlist
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable {
                                state.pickPlaylist(playlist.id)
                                onClose()
                            }
                            .padding(horizontal = 22.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        GradientArt(
                            seed = playlist.id,
                            modifier = Modifier
                                .size(42.dp)
                                .clip(RoundedCornerShape(12.dp)),
                            glyphSize = 18.dp,
                        )
                        Spacer(Modifier.width(14.dp))
                        Text(
                            text = playlist.name,
                            style = MaterialTheme.typography.bodyLarge,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )
                        if (selected) {
                            MzIcon(
                                glyph = MzGlyph.Play,
                                size = 18.dp,
                                tint = MaterialTheme.colorScheme.primary,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun HistorySheet(state: PlayerState, onClose: () -> Unit) {
    Column(
        modifier = Modifier
            .navigationBarsPadding()
            .padding(bottom = 10.dp),
    ) {
        Text(
            text = stringResource(R.string.sheet_history),
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier.padding(horizontal = 22.dp, vertical = 8.dp),
        )
        val entries = state.history
        if (entries.isEmpty()) {
            Text(
                text = stringResource(R.string.empty_history),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(22.dp),
            )
        } else {
            LazyColumn {
                items(entries.size) { index ->
                    val track = entries[index]
                    TrackRow(
                        track = track,
                        seed = track.id,
                        onClick = {
                            state.replay(track)
                            onClose()
                        },
                    )
                }
            }
        }
    }
}
