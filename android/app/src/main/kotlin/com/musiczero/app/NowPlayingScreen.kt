package com.musiczero.app

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.basicMarquee
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.util.Locale

/**
 * Full-screen now playing: the cover on a blurred copy of itself, an accent
 * color pulled out of the artwork, drag-down to close — the Retro layout.
 */
@Composable
fun NowPlayingScreen(
    state: PlayerState,
    onClose: () -> Unit,
    onOpenPlaylists: () -> Unit,
    onOpenHistory: () -> Unit,
    onOpenLibrary: () -> Unit,
) {
    val status = state.status
    var accent by remember { mutableStateOf<Color?>(null) }
    var backdrop by remember { mutableStateOf<ImageBitmap?>(null) }
    LaunchedEffect(status?.artUrl) {
        val url = status?.artUrl.orEmpty()
        accent = if (url.isEmpty()) null else Artwork.accent(url)
        backdrop = if (url.isEmpty()) null else Artwork.load(url)?.let { Artwork.soften(it) }
    }
    val accentColor = accent ?: MaterialTheme.colorScheme.primary
    val onAccent = if (accentColor.luminance() > 0.55f) Color.Black else Color.White
    val background = MaterialTheme.colorScheme.background

    var dragged by remember { mutableStateOf(0f) }
    val threshold = with(LocalDensity.current) { 130.dp.toPx() }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(background),
    ) {
        backdrop?.let { image ->
            Image(
                bitmap = image,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                filterQuality = FilterQuality.Low,
                alpha = 0.75f,
                modifier = Modifier
                    .fillMaxSize()
                    .scale(1.4f),
            )
        }
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(
                    Brush.verticalGradient(
                        colorStops = arrayOf(
                            0f to background.copy(alpha = 0.40f),
                            0.45f to background.copy(alpha = 0.70f),
                            1f to background.copy(alpha = 0.97f),
                        ),
                    ),
                ),
        )
        Column(
            modifier = Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .navigationBarsPadding()
                .padding(horizontal = 24.dp)
                .draggable(
                    orientation = Orientation.Vertical,
                    state = rememberDraggableState { delta ->
                        if (delta > 0f) {
                            dragged += delta
                        } else {
                            dragged = 0f
                        }
                        if (dragged > threshold) {
                            dragged = 0f
                            onClose()
                        }
                    },
                    onDragStopped = { dragged = 0f },
                ),
        ) {
            TopBar(status = status, onClose = onClose, onOpenPlaylists = onOpenPlaylists)
            Spacer(Modifier.height(10.dp))
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                contentAlignment = Alignment.Center,
            ) {
                ArtImage(
                    url = status?.artUrl.orEmpty(),
                    seed = status?.trackId?.takeIf { it.isNotEmpty() }
                        ?: status?.module
                        ?: "MusicZero",
                    modifier = Modifier
                        .fillMaxWidth()
                        .aspectRatio(1f)
                        .shadow(22.dp, RoundedCornerShape(28.dp)),
                    glyphSize = 92.dp,
                    cornerRadius = 28.dp,
                )
            }
            Spacer(Modifier.height(22.dp))
            Text(
                text = status?.title?.takeIf { it.isNotEmpty() }
                    ?: stringResource(R.string.nothing_playing),
                style = MaterialTheme.typography.headlineSmall,
                maxLines = 1,
                modifier = Modifier.basicMarquee(),
            )
            Text(
                text = status?.artist.orEmpty(),
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.padding(top = 4.dp),
            )
            Spacer(Modifier.height(16.dp))
            Timeline(state = state, accent = accentColor)
            Spacer(Modifier.height(8.dp))
            TransportRow(state = state, accent = accentColor, onAccent = onAccent)
            Spacer(Modifier.height(6.dp))
            ActionRow(
                state = state,
                onOpenHistory = onOpenHistory,
                onOpenLibrary = onOpenLibrary,
            )
            Spacer(Modifier.height(8.dp))
        }
    }
}

@Composable
private fun TopBar(status: Playing?, onClose: () -> Unit, onOpenPlaylists: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        GlyphButton(MzGlyph.ChevronDown, onClick = onClose, glyphSize = 26.dp)
        Column(
            modifier = Modifier
                .weight(1f)
                .padding(horizontal = 8.dp),
        ) {
            Text(
                text = stringResource(R.string.playing_from).uppercase(),
                style = MaterialTheme.typography.labelSmall.copy(letterSpacing = 1.6.sp),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
            Text(
                text = status?.playlist?.takeIf { it.isNotEmpty() }
                    ?: status?.name.orEmpty(),
                style = MaterialTheme.typography.titleMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        GlyphButton(MzGlyph.Queue, onClick = onOpenPlaylists, glyphSize = 24.dp)
    }
}

@Composable
private fun GlyphButton(
    glyph: MzGlyph,
    onClick: () -> Unit,
    size: Dp = 46.dp,
    glyphSize: Dp = 26.dp,
    tint: Color = LocalContentColor.current,
    enabled: Boolean = true,
) {
    Box(
        modifier = Modifier
            .size(size)
            .clip(CircleShape)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        MzIcon(
            glyph = glyph,
            size = glyphSize,
            tint = if (enabled) tint else tint.copy(alpha = 0.35f),
        )
    }
}

@Composable
private fun Timeline(state: PlayerState, accent: Color) {
    val status = state.status
    val duration = status?.durationMs ?: 0L
    val position = status?.positionMs ?: 0L
    val canSeek = status?.canSeek == true && duration > 0
    var dragging by remember { mutableStateOf<Float?>(null) }
    val fraction = dragging ?: if (duration > 0) position.toFloat() / duration else 0f
    val shown = if (dragging != null) ((dragging ?: 0f) * duration).toLong() else position
    Column {
        if (canSeek) {
            Slider(
                value = fraction.coerceIn(0f, 1f),
                onValueChange = { dragging = it },
                onValueChangeFinished = {
                    dragging?.let { state.seekTo((it * duration).toLong()) }
                    dragging = null
                },
                colors = SliderDefaults.colors(
                    thumbColor = accent,
                    activeTrackColor = accent,
                    inactiveTrackColor = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.2f),
                ),
            )
        } else {
            ProgressLine(
                fraction = fraction.coerceIn(0f, 1f),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 10.dp, vertical = 10.dp),
                height = 4.dp,
                color = accent,
            )
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 4.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                text = formatClock(shown),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                text = if (duration > 0) formatClock(duration) else stringResource(R.string.live_stream),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun TransportRow(state: PlayerState, accent: Color, onAccent: Color) {
    val playing = state.status?.status == "Playing"
    val canPrevious = state.status?.canPrevious == true
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        GlyphButton(
            MzGlyph.Prev,
            onClick = { state.previous() },
            size = 62.dp,
            glyphSize = 36.dp,
            enabled = canPrevious,
        )
        Spacer(Modifier.width(20.dp))
        Box(
            modifier = Modifier
                .size(86.dp)
                .clip(CircleShape)
                .background(accent)
                .clickable { state.toggle() },
            contentAlignment = Alignment.Center,
        ) {
            MzIcon(
                glyph = if (playing) MzGlyph.Pause else MzGlyph.Play,
                size = 40.dp,
                tint = onAccent,
            )
        }
        Spacer(Modifier.width(20.dp))
        GlyphButton(
            MzGlyph.Next,
            onClick = { state.next() },
            size = 62.dp,
            glyphSize = 36.dp,
        )
    }
}

@Composable
private fun ActionRow(
    state: PlayerState,
    onOpenHistory: () -> Unit,
    onOpenLibrary: () -> Unit,
) {
    val canSeek = state.status?.canSeek == true
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceEvenly,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        GlyphButton(
            MzGlyph.Clock,
            onClick = onOpenHistory,
            size = 52.dp,
            glyphSize = 25.dp,
            tint = muted,
        )
        SeekSecondsButton(forward = false, enabled = canSeek) { state.seekBy(-15) }
        SeekSecondsButton(forward = true, enabled = canSeek) { state.seekBy(15) }
        GlyphButton(
            MzGlyph.Note,
            onClick = onOpenLibrary,
            size = 52.dp,
            glyphSize = 25.dp,
            tint = muted,
        )
    }
}

/** Circular arrow with the number 15 inside, like Retro's seek buttons. */
@Composable
private fun SeekSecondsButton(forward: Boolean, enabled: Boolean, onClick: () -> Unit) {
    val measurer = rememberTextMeasurer()
    val tint = MaterialTheme.colorScheme.onSurfaceVariant
    val color = if (enabled) tint else tint.copy(alpha = 0.35f)
    Box(
        modifier = Modifier
            .size(52.dp)
            .clip(CircleShape)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.size(30.dp)) {
            circularArrow(color = color, forward = forward)
            val layout = measurer.measure(
                AnnotatedString("15"),
                style = TextStyle(
                    color = color,
                    fontSize = 9.sp,
                    fontWeight = FontWeight.Bold,
                ),
            )
            drawText(
                textLayoutResult = layout,
                topLeft = Offset(
                    center.x - layout.size.width / 2f,
                    center.y - layout.size.height / 2f,
                ),
            )
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
