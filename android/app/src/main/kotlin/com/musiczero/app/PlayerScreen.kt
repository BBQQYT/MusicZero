@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package com.musiczero.app

import android.content.Context
import android.graphics.BitmapFactory
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.URL
import java.util.Locale

/**
 * The single app screen. It talks straight to the engine socket; the
 * MediaLibraryService mirrors the same state for lock screen and Auto.
 */
@Composable
fun MusicZeroScreen() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val state = remember(context) { EngineState(context) }
    var playlistsSheet by remember { mutableStateOf(false) }
    var historySheet by remember { mutableStateOf(false) }
    var tokenSheet by remember { mutableStateOf(false) }

    LaunchedEffect(state) {
        state.beginPolling(scope)
    }

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            ModuleRow(state)
            CoverPanel(state)
            TrackHeading(state)
            PositionBar(state)
            TransportRow(state)
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                CardButton(
                    stringResource(R.string.action_playlist),
                    modifier = Modifier.weight(1f),
                    enabled = state.playlists.playlists.isNotEmpty(),
                ) { playlistsSheet = true }
                CardButton(
                    stringResource(R.string.action_history),
                    modifier = Modifier.weight(1f),
                ) { historySheet = true }
                CardButton(
                    stringResource(R.string.action_token),
                    modifier = Modifier.weight(1f),
                ) { tokenSheet = true }
            }
            ConnectionFooter(state)
        }
    }

    if (playlistsSheet) {
        ModalBottomSheet(onDismissRequest = { playlistsSheet = false }) {
            ListSheet(
                title = stringResource(R.string.action_playlist),
                emptyMessage = stringResource(R.string.no_playlists),
                rows = state.playlists.playlists.map { it.name },
                selectedText = state.status?.playlist,
            ) { index ->
                state.playlists.playlists.getOrNull(index)?.let { state.pickPlaylist(it.id) }
                playlistsSheet = false
            }
        }
    }
    if (historySheet) {
        ModalBottomSheet(onDismissRequest = { historySheet = false }) {
            ListSheet(
                title = stringResource(R.string.action_history),
                emptyMessage = stringResource(R.string.empty_history),
                rows = state.history.map { artistTitle(it) },
                selectedText = null,
            ) { index ->
                state.history.getOrNull(index)?.let { state.replay(it) }
                historySheet = false
            }
        }
    }
    if (tokenSheet) {
        ModalBottomSheet(onDismissRequest = { tokenSheet = false }) {
            TokenSheet(context) { tokenSheet = false }
        }
    }
}

private fun artistTitle(track: Track): String =
    (if (track.artist.isNotEmpty()) track.artist + " — " else "") + track.title

@Composable
private fun ConnectionFooter(state: EngineState) {
    val active = state.status
    val text = when {
        state.modules.isEmpty() -> stringResource(R.string.no_modules)
        state.connection == Connection.Starting -> stringResource(R.string.engine_starting)
        state.connection == Connection.Down -> stringResource(R.string.engine_down)
        active == null -> stringResource(R.string.engine_down)
        else -> active.module
    }
    Text(
        text = text,
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(bottom = 16.dp),
    )
}

@Composable
private fun ModuleRow(state: EngineState) {
    if (state.modules.isEmpty()) {
        return
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        state.modules.forEach { module ->
            AssistChip(
                onClick = { state.switchTo(module.id) },
                label = { Text(module.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                leadingIcon = if (state.status?.module == module.id) {
                    {
                        Box(
                            modifier = Modifier
                                .size(10.dp)
                                .background(MaterialTheme.colorScheme.primary, RoundedCornerShape(50)),
                        )
                    }
                } else {
                    null
                },
            )
        }
    }
}

@Composable
private fun CoverPanel(state: EngineState) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(300.dp)
            .clip(RoundedCornerShape(24.dp))
            .background(MaterialTheme.colorScheme.surfaceContainer),
        contentAlignment = Alignment.Center,
    ) {
        val status = state.status
        when {
            status == null -> CircularProgressIndicator(
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(40.dp),
            )
            else -> RemoteArt(status.artUrl)
        }
    }
}

@Composable
private fun RemoteArt(url: String) {
    var bitmap by remember(url) { mutableStateOf<android.graphics.Bitmap?>(null) }
    if (url.isNotEmpty()) {
        LaunchedEffect(url) {
            bitmap = withContext(Dispatchers.IO) {
                try {
                    URL(url).openStream().use { stream ->
                        BitmapFactory.decodeStream(stream)
                    }
                } catch (_: Exception) {
                    null
                }
            }
        }
    }
    val loaded = bitmap
    when {
        loaded != null -> Image(
            bitmap = loaded.asImageBitmap(),
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = Modifier.fillMaxSize(),
        )
        url.isEmpty() -> {
            val tint = MaterialTheme.colorScheme.primary
            Canvas(Modifier.size(84.dp)) { playGlyph(this, tint, soft = true) }
        }
    }
}

@Composable
private fun TrackHeading(state: EngineState) {
    val status = state.status
    if (status == null) {
        Text(
            text = stringResource(R.string.nothing_playing),
            style = MaterialTheme.typography.titleMedium,
        )
        return
    }
    Text(
        text = status.title.ifEmpty { status.name },
        style = MaterialTheme.typography.headlineSmall,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
    Text(
        text = status.artist,
        style = MaterialTheme.typography.bodyLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

@Composable
private fun PositionBar(state: EngineState) {
    val status = state.status
    val duration = status?.durationMs ?: 0L
    val position = status?.positionMs ?: 0L
    var dragging by remember { mutableStateOf<Float?>(null) }
    val fraction = dragging ?: if (duration > 0) position.toFloat() / duration else 0f
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        if ((status?.canSeek == true) && duration > 0) {
            Slider(
                value = fraction.coerceIn(0f, 1f),
                onValueChange = { dragging = it },
                onValueChangeFinished = {
                    dragging?.let { value ->
                        state.seekTo((value * duration).toLong())
                    }
                    dragging = null
                },
            )
        } else {
            LinearStrip(fraction.coerceIn(0f, 1f))
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 4.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                text = formatClock(
                    if (dragging != null) ((dragging ?: 0f) * duration).toLong() else position
                ),
                style = MaterialTheme.typography.labelSmall,
            )
            Text(formatClock(duration), style = MaterialTheme.typography.labelSmall)
        }
    }
}

private fun formatClock(milliseconds: Long): String {
    if (milliseconds <= 0) {
        return "0:00"
    }
    val total = milliseconds / 1000
    val minutes = total / 60
    val seconds = total % 60
    return String.format(Locale.ROOT, "%d:%02d", minutes, seconds)
}

@Composable
private fun TransportRow(state: EngineState) {
    val playing = state.status?.status == "Playing"
    val canPrevious = state.status?.canPrevious == true
    val onSurface = MaterialTheme.colorScheme.onSurface
    val onPrimary = MaterialTheme.colorScheme.onPrimary
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TextButton(onClick = { state.seekBy(-15) }) { Text("−15s") }
        FilledTonalIconButton(
            onClick = { state.previous() },
            enabled = canPrevious,
            modifier = Modifier.size(64.dp),
        ) {
            Glyph(36.dp) { skipPreviousGlyph(this, onSurface) }
        }
        FilledTonalIconButton(
            onClick = { state.toggle() },
            colors = IconButtonDefaults.filledTonalIconButtonColors(
                containerColor = MaterialTheme.colorScheme.primary,
            ),
            modifier = Modifier
                .padding(horizontal = 14.dp)
                .size(84.dp),
        ) {
            if (playing) {
                Glyph(40.dp) { pauseGlyph(this, onPrimary) }
            } else {
                Glyph(40.dp) { playGlyph(this, onPrimary) }
            }
        }
        FilledTonalIconButton(
            onClick = { state.next() },
            modifier = Modifier.size(64.dp),
        ) {
            Glyph(36.dp) { skipNextGlyph(this, onSurface) }
        }
        TextButton(onClick = { state.seekBy(15) }) { Text("+15s") }
    }
}

@Composable
private fun CardButton(
    label: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    FilledTonalIconButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.defaultMinSize(minWidth = 72.dp, minHeight = 52.dp),
    ) {
        Text(label, style = MaterialTheme.typography.labelMedium, textAlign = TextAlign.Center)
    }
}

@Composable
private fun Glyph(size: Dp, paint: DrawScope.() -> Unit) {
    Canvas(modifier = Modifier.size(size)) {
        paint()
    }
}

@Composable
private fun LinearStrip(fraction: Float) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(6.dp)
            .clip(RoundedCornerShape(3.dp))
            .background(MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth(fraction)
                .height(6.dp)
                .background(MaterialTheme.colorScheme.primary),
        )
    }
}

/**
 * Common sheet body: a scrollable list of plain text rows with an optional
 * highlight for the selected entry.
 */
@Composable
private fun ListSheet(
    title: String,
    emptyMessage: String,
    rows: List<String>,
    selectedText: String?,
    onClick: (Int) -> Unit,
) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleLarge,
        modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
    )
    if (rows.isEmpty()) {
        Text(
            text = emptyMessage,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(24.dp),
        )
        return
    }
    LazyColumn(modifier = Modifier.padding(bottom = 24.dp)) {
        items(rows.size) { index ->
            val row = rows[index]
            val selected = row == selectedText
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { onClick(index) }
                    .padding(horizontal = 24.dp, vertical = 12.dp),
            ) {
                Text(
                    text = row,
                    style = MaterialTheme.typography.bodyLarge,
                    color = if (selected) {
                        MaterialTheme.colorScheme.primary
                    } else {
                        MaterialTheme.colorScheme.onSurface
                    },
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun TokenSheet(context: Context, onClose: () -> Unit) {
    var token by remember { mutableStateOf(readToken(context)) }
    Column(
        modifier = Modifier.padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(stringResource(R.string.action_token), style = MaterialTheme.typography.titleLarge)
        OutlinedTextField(
            value = token,
            onValueChange = { token = it },
            label = { Text(stringResource(R.string.token_hint)) },
            minLines = 3,
            maxLines = 6,
            modifier = Modifier.fillMaxWidth(),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(
                modifier = Modifier.weight(1f),
                enabled = token.isNotBlank(),
                onClick = {
                    saveToken(context, token.trim())
                    onClose()
                },
            ) { Text(stringResource(R.string.save)) }
            TextButton(
                modifier = Modifier.weight(1f),
                onClick = onClose,
            ) { Text(stringResource(R.string.cancel)) }
        }
    }
}

private fun readToken(context: Context): String = try {
    Engine.configDir(context).resolve("ymz/token").takeIf { it.isFile }?.readText()?.trim() ?: ""
} catch (_: Exception) {
    ""
}

private fun saveToken(context: Context, token: String) {
    try {
        Engine.configDir(context).resolve("ymz").apply { mkdirs() }
            .resolve("token").writeText(token)
    } catch (_: Exception) {
    }
}

private fun skipNextGlyph(scope: DrawScope, color: Color) = with(scope) {
    val width = size.width
    val height = size.height
    val triangle = Path().apply {
        moveTo(width * 0.16f, height * 0.18f)
        lineTo(width * 0.58f, height * 0.50f)
        lineTo(width * 0.16f, height * 0.82f)
        close()
    }
    clipPath(triangle) { drawPath(triangle, color) }
    drawRoundRect(
        color = color,
        topLeft = Offset(width * 0.68f, height * 0.16f),
        size = Size(width * 0.14f, height * 0.68f),
        cornerRadius = CornerRadius(width * 0.05f),
    )
}

private fun skipPreviousGlyph(scope: DrawScope, color: Color) = with(scope) {
    val width = size.width
    val height = size.height
    val triangle = Path().apply {
        moveTo(width * 0.84f, height * 0.18f)
        lineTo(width * 0.42f, height * 0.50f)
        lineTo(width * 0.84f, height * 0.82f)
        close()
    }
    clipPath(triangle) { drawPath(triangle, color) }
    drawRoundRect(
        color = color,
        topLeft = Offset(width * 0.18f, height * 0.16f),
        size = Size(width * 0.14f, height * 0.68f),
        cornerRadius = CornerRadius(width * 0.05f),
    )
}

fun playGlyph(scope: DrawScope, color: Color, soft: Boolean = false) = with(scope) {
    val triangle = Path().apply {
        moveTo(size.width * 0.28f, size.height * 0.16f)
        lineTo(size.width * 0.82f, size.height * 0.50f)
        lineTo(size.width * 0.28f, size.height * 0.84f)
        close()
    }
    scope.drawPath(triangle, color.copy(alpha = if (soft) 0.6f else 1f))
}

fun pauseGlyph(scope: DrawScope, color: Color) = with(scope) {
    drawRoundRect(
        color = color,
        topLeft = Offset(size.width * 0.22f, size.height * 0.16f),
        size = Size(size.width * 0.20f, size.height * 0.68f),
        cornerRadius = CornerRadius(size.width * 0.07f),
    )
    drawRoundRect(
        color = color,
        topLeft = Offset(size.width * 0.58f, size.height * 0.16f),
        size = Size(size.width * 0.20f, size.height * 0.68f),
        cornerRadius = CornerRadius(size.width * 0.07f),
    )
}
