package com.musiczero.app

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.URL

/**
 * Artwork cache plus the two color tricks the Retro-style UI leans on:
 * a dominant color extracted from the cover (now-playing accent) and a tiny
 * downscaled copy that the Image composable stretches back up, which gives a
 * soft blur on every API level without RenderEffect.
 */
object Artwork {

    private val bitmaps = object : LinkedHashMap<String, Bitmap>(24, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, Bitmap>): Boolean = size > 36
    }

    private val accents = object : LinkedHashMap<String, Color>(36, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, Color>): Boolean = size > 48
    }

    private val gradients = object : LinkedHashMap<String, List<Color>>(24, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, List<Color>>): Boolean = size > 64
    }

    suspend fun load(url: String): Bitmap? {
        if (url.isEmpty()) {
            return null
        }
        synchronized(bitmaps) { bitmaps[url] }?.let { return it }
        val decoded = withContext(Dispatchers.IO) { decode(url) } ?: return null
        synchronized(bitmaps) { bitmaps[url] = decoded }
        return decoded
    }

    /** Dominant vibrant color of the cover; null when the art is unusable. */
    suspend fun accent(url: String): Color? {
        if (url.isEmpty()) {
            return null
        }
        synchronized(accents) { accents[url] }?.let { return it }
        val bitmap = load(url) ?: return null
        val color = withContext(Dispatchers.Default) { dominant(bitmap) } ?: return null
        synchronized(accents) { accents[url] = color }
        return color
    }

    /** Deterministic colorful gradient for entries that have no artwork. */
    fun gradient(seed: String): List<Color> = synchronized(gradients) {
        gradients.getOrPut(seed.ifEmpty { "MusicZero" }) {
            val hue = (((seed.ifEmpty { "MusicZero" }.hashCode() % 360) + 360) % 360).toFloat()
            listOf(
                Color.hsv(hue, 0.52f, 0.72f),
                Color.hsv((hue + 38f) % 360f, 0.70f, 0.36f),
            )
        }
    }

    /** Tiny copy stretched by the Image composable: the poor-man's blur. */
    fun soften(bitmap: Bitmap, width: Int = 26): ImageBitmap {
        val height = (width.toFloat() * bitmap.height / bitmap.width).toInt().coerceAtLeast(8)
        return Bitmap.createScaledBitmap(bitmap, width, height, true).asImageBitmap()
    }

    private fun decode(url: String): Bitmap? = try {
        val bytes = URL(url).openStream().use { input -> input.readBytes() }
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        var sample = 1
        while (bounds.outWidth / (sample * 2) >= 512 && bounds.outHeight / (sample * 2) >= 512) {
            sample *= 2
        }
        val options = BitmapFactory.Options().apply { inSampleSize = sample }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
    } catch (_: Exception) {
        null
    }

    private fun dominant(bitmap: Bitmap): Color? {
        val small = Bitmap.createScaledBitmap(bitmap, 32, 32, true)
        val hsv = FloatArray(3)
        val weight = FloatArray(12)
        val red = FloatArray(12)
        val green = FloatArray(12)
        val blue = FloatArray(12)
        val count = IntArray(12)
        for (y in 0 until 32) {
            for (x in 0 until 32) {
                val pixel = small.getPixel(x, y)
                if (android.graphics.Color.alpha(pixel) < 200) {
                    continue
                }
                val r = android.graphics.Color.red(pixel) / 255f
                val g = android.graphics.Color.green(pixel) / 255f
                val b = android.graphics.Color.blue(pixel) / 255f
                android.graphics.Color.colorToHSV(pixel, hsv)
                val saturation = hsv[1]
                val value = hsv[2]
                if (value < 0.12f) {
                    continue
                }
                val bucket = (hsv[0] / 30f).toInt().coerceIn(0, 11)
                val w = saturation * value
                weight[bucket] += w
                red[bucket] += r * w
                green[bucket] += g * w
                blue[bucket] += b * w
                count[bucket]++
            }
        }
        var best = -1
        var bestWeight = 0f
        for (index in 0..11) {
            if (weight[index] > bestWeight) {
                bestWeight = weight[index]
                best = index
            }
        }
        if (best < 0 || count[best] == 0) {
            return null
        }
        val color = Color(
            red = (red[best] / weight[best]).coerceIn(0f, 1f),
            green = (green[best] / weight[best]).coerceIn(0f, 1f),
            blue = (blue[best] / weight[best]).coerceIn(0f, 1f),
        )
        // Push slightly toward vibrancy so the accent stays readable on dark.
        android.graphics.Color.colorToHSV(
            android.graphics.Color.rgb(
                (color.red * 255).toInt(),
                (color.green * 255).toInt(),
                (color.blue * 255).toInt(),
            ),
            hsv,
        )
        return Color.hsv(
            hue = hsv[0],
            saturation = hsv[1].coerceAtLeast(0.45f),
            value = hsv[2].coerceIn(0.62f, 0.92f),
        )
    }
}
