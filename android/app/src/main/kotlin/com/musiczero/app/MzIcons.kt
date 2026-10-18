package com.musiczero.app

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.cos
import kotlin.math.sin

/**
 * Hand-drawn icon set. The app ships no material-icons artifact: every glyph
 * is a handful of shapes on a Canvas, so a new icon costs a few lines instead
 * of a megabyte of vectors.
 */
enum class MzGlyph {
    Home, Grid, Clock, Tune,
    Play, Pause, Next, Prev,
    ChevronDown, Note, Queue, Replay,
    Check, Close, Stop, More,
}

@Composable
fun MzIcon(
    glyph: MzGlyph,
    modifier: Modifier = Modifier,
    size: Dp = 24.dp,
    tint: Color = LocalContentColor.current,
) = Canvas(modifier.size(size)) { drawGlyph(glyph, tint) }

fun DrawScope.drawGlyph(glyph: MzGlyph, color: Color) {
    when (glyph) {
        MzGlyph.Play -> triangle(Offset(0.30f, 0.16f), Offset(0.82f, 0.50f), Offset(0.30f, 0.84f), color)
        MzGlyph.Pause -> {
            bar(0.24f, 0.20f, 0.18f, 0.60f, color)
            bar(0.58f, 0.20f, 0.18f, 0.60f, color)
        }
        MzGlyph.Next -> {
            triangle(Offset(0.10f, 0.18f), Offset(0.56f, 0.50f), Offset(0.10f, 0.82f), color)
            bar(0.66f, 0.18f, 0.15f, 0.64f, color)
        }
        MzGlyph.Prev -> {
            triangle(Offset(0.90f, 0.18f), Offset(0.44f, 0.50f), Offset(0.90f, 0.82f), color)
            bar(0.19f, 0.18f, 0.15f, 0.64f, color)
        }
        MzGlyph.Home -> {
            val path = Path().apply {
                moveTo(size.width * 0.50f, size.height * 0.10f)
                lineTo(size.width * 0.92f, size.height * 0.44f)
                lineTo(size.width * 0.81f, size.height * 0.44f)
                lineTo(size.width * 0.81f, size.height * 0.88f)
                lineTo(size.width * 0.19f, size.height * 0.88f)
                lineTo(size.width * 0.19f, size.height * 0.44f)
                lineTo(size.width * 0.08f, size.height * 0.44f)
                close()
            }
            fillRounded(path, color, radius = size.minDimension * 0.045f)
        }
        MzGlyph.Grid -> {
            val cell = size.width * 0.34f
            val radius = CornerRadius(cell * 0.34f)
            for (x in 0..1) {
                for (y in 0..1) {
                    drawRoundRect(
                        color = color,
                        topLeft = Offset(size.width * (0.12f + 0.42f * x), size.height * (0.12f + 0.42f * y)),
                        size = Size(cell, cell),
                        cornerRadius = radius,
                    )
                }
            }
        }
        MzGlyph.Clock -> {
            val stroke = size.minDimension * 0.09f
            val radius = size.minDimension * 0.40f
            drawCircle(color, radius = radius, center = center, style = Stroke(stroke))
            line(Offset(0.50f, 0.50f), Offset(0.50f, 0.28f), color, stroke)
            line(Offset(0.50f, 0.50f), Offset(0.69f, 0.58f), color, stroke)
        }
        MzGlyph.Tune -> {
            val stroke = size.minDimension * 0.075f
            line(Offset(0.12f, 0.26f), Offset(0.88f, 0.26f), color, stroke)
            line(Offset(0.12f, 0.50f), Offset(0.88f, 0.50f), color, stroke)
            line(Offset(0.12f, 0.74f), Offset(0.88f, 0.74f), color, stroke)
            dot(0.36f, 0.26f, 0.115f, color)
            dot(0.62f, 0.50f, 0.115f, color)
            dot(0.40f, 0.74f, 0.115f, color)
        }
        MzGlyph.ChevronDown -> {
            val stroke = size.minDimension * 0.11f
            line(Offset(0.22f, 0.36f), Offset(0.50f, 0.66f), color, stroke)
            line(Offset(0.50f, 0.66f), Offset(0.78f, 0.36f), color, stroke)
        }
        MzGlyph.Note -> {
            drawCircle(color, radius = size.minDimension * 0.17f, center = Offset(size.width * 0.32f, size.height * 0.75f))
            drawRoundRect(
                color = color,
                topLeft = Offset(size.width * 0.45f, size.height * 0.20f),
                size = Size(size.width * 0.09f, size.height * 0.55f),
                cornerRadius = CornerRadius(size.width * 0.045f),
            )
            val flag = Path().apply {
                moveTo(size.width * 0.52f, size.height * 0.20f)
                lineTo(size.width * 0.82f, size.height * 0.29f)
                lineTo(size.width * 0.82f, size.height * 0.45f)
                lineTo(size.width * 0.52f, size.height * 0.36f)
                close()
            }
            drawPath(flag, color)
        }
        MzGlyph.Queue -> {
            val stroke = size.minDimension * 0.095f
            line(Offset(0.14f, 0.24f), Offset(0.86f, 0.24f), color, stroke)
            line(Offset(0.14f, 0.50f), Offset(0.86f, 0.50f), color, stroke)
            line(Offset(0.14f, 0.76f), Offset(0.62f, 0.76f), color, stroke)
        }
        MzGlyph.Replay -> circularArrow(color, forward = false)
        MzGlyph.Check -> {
            val stroke = size.minDimension * 0.13f
            line(Offset(0.20f, 0.54f), Offset(0.42f, 0.76f), color, stroke)
            line(Offset(0.42f, 0.76f), Offset(0.82f, 0.28f), color, stroke)
        }
        MzGlyph.Close -> {
            val stroke = size.minDimension * 0.11f
            line(Offset(0.26f, 0.26f), Offset(0.74f, 0.74f), color, stroke)
            line(Offset(0.74f, 0.26f), Offset(0.26f, 0.74f), color, stroke)
        }
        MzGlyph.Stop -> drawRoundRect(
            color = color,
            topLeft = Offset(size.width * 0.24f, size.height * 0.24f),
            size = Size(size.width * 0.52f, size.height * 0.52f),
            cornerRadius = CornerRadius(size.minDimension * 0.12f),
        )
        MzGlyph.More -> {
            dot(0.5f, 0.20f, 0.085f, color)
            dot(0.5f, 0.50f, 0.085f, color)
            dot(0.5f, 0.80f, 0.085f, color)
        }
    }
}

/** Counter-clockwise refresh arrow used for history and seek buttons. */
fun DrawScope.circularArrow(color: Color, forward: Boolean) {
    val m = size.minDimension
    val radius = m * 0.34f
    val stroke = m * 0.10f
    drawArc(
        color = color,
        startAngle = 300f,
        sweepAngle = 300f,
        useCenter = false,
        topLeft = Offset(center.x - radius, center.y - radius),
        size = Size(radius * 2f, radius * 2f),
        style = Stroke(width = stroke, cap = StrokeCap.Round),
    )
    val angle = if (forward) 240.0 else 300.0
    val radians = Math.toRadians(angle)
    val tip = Offset(
        center.x + radius * cos(radians).toFloat(),
        center.y + radius * sin(radians).toFloat(),
    )
    val direction = if (forward) {
        Offset(-sin(radians).toFloat(), cos(radians).toFloat())
    } else {
        Offset(sin(radians).toFloat(), -cos(radians).toFloat())
    }
    arrowHead(tip, direction, m * 0.20f, color)
}

fun DrawScope.arrowHead(tip: Offset, direction: Offset, size: Float, color: Color) {
    val length = kotlin.math.sqrt(direction.x * direction.x + direction.y * direction.y)
    if (length <= 0f) {
        return
    }
    val dir = Offset(direction.x / length, direction.y / length)
    val perp = Offset(-dir.y, dir.x)
    val half = size * 0.62f
    val base = Offset(tip.x - dir.x * size, tip.y - dir.y * size)
    val path = Path().apply {
        moveTo(tip.x, tip.y)
        lineTo(base.x + perp.x * half, base.y + perp.y * half)
        lineTo(base.x - perp.x * half, base.y - perp.y * half)
        close()
    }
    drawPath(path, color)
    drawPath(path, color, style = Stroke(width = size * 0.3f, join = StrokeJoin.Round, cap = StrokeCap.Round))
}

/** Filled triangle with softly rounded corners. */
fun DrawScope.triangle(a: Offset, b: Offset, c: Offset, color: Color) {
    val path = Path().apply {
        moveTo(size.width * a.x, size.height * a.y)
        lineTo(size.width * b.x, size.height * b.y)
        lineTo(size.width * c.x, size.height * c.y)
        close()
    }
    fillRounded(path, color, radius = size.minDimension * 0.07f)
}

private fun DrawScope.fillRounded(path: Path, color: Color, radius: Float) {
    drawPath(path, color)
    drawPath(path, color, style = Stroke(width = radius * 2f, join = StrokeJoin.Round, cap = StrokeCap.Round))
}

private fun DrawScope.bar(x: Float, y: Float, width: Float, height: Float, color: Color) {
    drawRoundRect(
        color = color,
        topLeft = Offset(size.width * x, size.height * y),
        size = Size(size.width * width, size.height * height),
        cornerRadius = CornerRadius(size.minDimension * 0.07f),
    )
}

private fun DrawScope.line(a: Offset, b: Offset, color: Color, stroke: Float) {
    drawLine(
        color = color,
        start = Offset(size.width * a.x, size.height * a.y),
        end = Offset(size.width * b.x, size.height * b.y),
        strokeWidth = stroke,
        cap = StrokeCap.Round,
    )
}

private fun DrawScope.dot(x: Float, y: Float, radius: Float, color: Color) {
    drawCircle(color, radius = size.minDimension * radius, center = Offset(size.width * x, size.height * y))
}
