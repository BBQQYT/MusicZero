@file:OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)

package com.musiczero.app

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.basicMarquee
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
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
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Remote artwork with a generated gradient standing in until it arrives. */
@Composable
fun ArtImage(
    url: String,
    seed: String,
    modifier: Modifier = Modifier,
    glyphSize: Dp = 40.dp,
    cornerRadius: Dp = 16.dp,
) {
    var bitmap by remember(url) { mutableStateOf<android.graphics.Bitmap?>(null) }
    LaunchedEffect(url) {
        bitmap = if (url.isEmpty()) null else Artwork.load(url)
    }
    val loaded = bitmap
    val shape = RoundedCornerShape(cornerRadius)
    if (loaded != null) {
        Image(
            bitmap = loaded.asImageBitmap(),
            contentDescription = null,
            contentScale = ContentScale.Crop,
            filterQuality = FilterQuality.Medium,
            modifier = modifier.clip(shape),
        )
    } else {
        GradientArt(seed = seed, modifier = modifier.clip(shape), glyphSize = glyphSize)
    }
}

/** Fallback cover: deterministic two-color gradient plus a note glyph. */
@Composable
fun GradientArt(
    seed: String,
    modifier: Modifier = Modifier,
    glyphSize: Dp = 40.dp,
) {
    val colors = remember(seed) { Artwork.gradient(seed) }
    Box(modifier.background(Brush.linearGradient(colors)), contentAlignment = Alignment.Center) {
        MzIcon(
            glyph = MzGlyph.Note,
            size = glyphSize,
            tint = Color.White.copy(alpha = 0.35f),
            modifier = Modifier.rotate(-10f),
        )
    }
}

@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier) {
    Text(
        text = text,
        style = MaterialTheme.typography.titleLarge,
        modifier = modifier.padding(horizontal = 20.dp, vertical = 6.dp),
    )
}

/** Wide colorful provider card, Home and Library headers both use it. */
@Composable
fun ModuleCard(
    module: Manifest,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = remember(module.id) { Artwork.gradient(module.id) }
    val shape = RoundedCornerShape(20.dp)
    Box(
        modifier = modifier
            .width(196.dp)
            .height(88.dp)
            .clip(shape)
            .background(Brush.linearGradient(colors))
            .clickable(onClick = onClick)
            .padding(14.dp),
    ) {
        Text(
            text = module.name,
            style = MaterialTheme.typography.titleMedium,
            color = Color.White,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.align(Alignment.TopStart),
        )
        Row(
            modifier = Modifier.align(Alignment.BottomStart),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(
                text = module.id,
                style = MaterialTheme.typography.labelSmall,
                color = Color.White.copy(alpha = 0.75f),
            )
            if (selected) {
                MzIcon(
                    glyph = MzGlyph.Check,
                    size = 16.dp,
                    tint = Color.White,
                )
            }
        }
    }
}

/** Big rounded playlist tile: gradient background, name over a scrim. */
@Composable
fun PlaylistCard(
    playlist: Playlist,
    selected: Boolean,
    playing: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = remember(playlist.id) { Artwork.gradient(playlist.id) }
    Box(
        modifier = modifier
            .aspectRatio(1f)
            .clip(RoundedCornerShape(22.dp))
            .background(Brush.linearGradient(colors))
            .clickable(onClick = onClick),
    ) {
        MzIcon(
            glyph = MzGlyph.Note,
            size = 64.dp,
            tint = Color.White.copy(alpha = 0.22f),
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(end = 8.dp, bottom = 2.dp)
                .rotate(-14f),
        )
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(110.dp)
                .align(Alignment.BottomStart)
                .background(
                    Brush.verticalGradient(
                        listOf(Color.Transparent, Color.Black.copy(alpha = 0.55f)),
                    ),
                ),
        )
        Text(
            text = playlist.name,
            style = MaterialTheme.typography.titleMedium,
            color = Color.White,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .align(Alignment.BottomStart)
                .padding(14.dp),
        )
        if (playing || selected) {
            Box(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(10.dp)
                    .size(28.dp)
                    .clip(CircleShape)
                    .background(
                        if (playing) MaterialTheme.colorScheme.primary
                        else Color.Black.copy(alpha = 0.45f),
                    ),
                contentAlignment = Alignment.Center,
            ) {
                MzIcon(
                    glyph = if (playing) MzGlyph.Play else MzGlyph.Check,
                    size = 15.dp,
                    tint = if (playing) {
                        MaterialTheme.colorScheme.onPrimary
                    } else {
                        Color.White
                    },
                )
            }
        }
    }
}

/** One history entry: rounded art, title, artist and an optional trailing slot. */
@Composable
fun TrackRow(
    track: Track,
    seed: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 20.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ArtImage(
            url = track.artUrl,
            seed = seed,
            modifier = Modifier.size(54.dp),
            glyphSize = 22.dp,
            cornerRadius = 14.dp,
        )
        Spacer(Modifier.width(14.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = track.title.ifEmpty { "—" },
                style = MaterialTheme.typography.bodyLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (track.artist.isNotEmpty()) {
                Text(
                    text = track.artist,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        trailing?.invoke()
    }
}

/** Floating mini player parked above the navigation bar. */
@Composable
fun MiniPlayer(
    status: Playing,
    onOpen: () -> Unit,
    onToggle: () -> Unit,
    onNext: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val playing = status.status == "Playing"
    Box(modifier = modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp)) {
        Surface(
            shape = RoundedCornerShape(22.dp),
            color = MaterialTheme.colorScheme.surfaceContainerHigh,
            shadowElevation = 8.dp,
            modifier = Modifier
                .fillMaxWidth()
                .clickable(onClick = onOpen),
        ) {
            Column {
                Row(
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 7.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    ArtImage(
                        url = status.artUrl,
                        seed = status.trackId.ifEmpty { status.module },
                        modifier = Modifier.size(46.dp),
                        glyphSize = 20.dp,
                        cornerRadius = 13.dp,
                    )
                    Spacer(Modifier.width(12.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = status.title.ifEmpty { status.name },
                            style = MaterialTheme.typography.titleMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.basicMarquee(),
                        )
                        Text(
                            text = status.artist.ifEmpty { status.playlist },
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    IconButton(onClick = onToggle, modifier = Modifier.size(46.dp)) {
                        MzIcon(
                            glyph = if (playing) MzGlyph.Pause else MzGlyph.Play,
                            size = 30.dp,
                        )
                    }
                    IconButton(onClick = onNext, modifier = Modifier.size(46.dp)) {
                        MzIcon(glyph = MzGlyph.Next, size = 26.dp)
                    }
                }
                ProgressLine(
                    fraction = status.progressFraction(),
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 10.dp)
                        .padding(bottom = 8.dp),
                )
            }
        }
    }
}

fun Playing.progressFraction(): Float =
    if (durationMs > 0) (positionMs.toFloat() / durationMs).coerceIn(0f, 1f) else 0f

/** Thin progress strip drawn by hand: no M3 signature surprises. */
@Composable
fun ProgressLine(
    fraction: Float,
    modifier: Modifier = Modifier,
    height: Dp = 3.dp,
    color: Color = MaterialTheme.colorScheme.primary,
    track: Color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.14f),
) {
    Box(
        modifier = modifier
            .height(height)
            .clip(CircleShape)
            .background(track),
    ) {
        if (fraction > 0f) {
            Box(
                modifier = Modifier
                    .fillMaxWidth(fraction.coerceIn(0f, 1f))
                    .height(height)
                    .background(color),
            )
        }
    }
}

/** Selectable pill used for provider filters. */
@Composable
fun PillChip(
    text: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        shape = CircleShape,
        color = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceContainerHigh,
        contentColor = if (selected) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = modifier.clickable(onClick = onClick),
    ) {
        Text(
            text = text,
            style = MaterialTheme.typography.labelLarge,
            maxLines = 1,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 9.dp),
        )
    }
}

@Composable
fun StatusDot(connection: Connection, modifier: Modifier = Modifier) {
    val color = when (connection) {
        Connection.Up -> Color(0xFF4ADE80)
        Connection.Starting -> Color(0xFFFBBF24)
        Connection.Down -> MaterialTheme.colorScheme.error
    }
    Box(
        modifier = modifier
            .size(10.dp)
            .clip(CircleShape)
            .background(color),
    )
}

@Composable
fun EmptyState(glyph: MzGlyph, text: String, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.fillMaxWidth().padding(vertical = 48.dp, horizontal = 32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        MzIcon(
            glyph = glyph,
            size = 54.dp,
            tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.45f),
        )
        Text(
            text = text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}
