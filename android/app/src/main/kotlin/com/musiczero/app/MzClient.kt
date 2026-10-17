package com.musiczero.app

import android.net.LocalSocket
import android.net.LocalSocketAddress
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.DataInputStream
import java.io.File

/**
 * The same one-line JSON control protocol the mz CLI and tray use. The host
 * listens on a unix socket under our cache directory; LocalSocket talks to it
 * directly, so no subprocess is spawned per command.
 */
class MzClient(private val socketPath: File) {

    var lastError: String? = null
        private set

    /** Sends one request line and parses one reply line. */
    suspend fun call(request: JSONObject): JSONObject? =
        withContext(Dispatchers.IO) {
            try {
                LocalSocket().use { socket ->
                    socket.connect(
                        LocalSocketAddress(socketPath.absolutePath, LocalSocketAddress.Namespace.FILESYSTEM)
                    )
                    socket.outputStream.apply {
                        write((request.toString() + "\n").toByteArray(Charsets.UTF_8))
                        flush()
                    }
                    val reader = DataInputStream(socket.inputStream)
                    readLine(reader)?.let { JSONObject(it) }
                }
            } catch (failure: Exception) {
                lastError = failure.message ?: failure.toString()
                null
            }
        }

    private fun readLine(reader: DataInputStream): String? {
        val bytes = ArrayList<Byte>(1024)
        while (true) {
            val value = try {
                reader.read()
            } catch (_: Exception) {
                return null
            }
            when {
                value < 0 -> return if (bytes.isEmpty()) null else String(bytes.toByteArray(), Charsets.UTF_8)
                value == '\n'.code -> return String(bytes.toByteArray(), Charsets.UTF_8)
                value != '\r'.code -> bytes.add(value.toByte())
            }
        }
    }

    companion object {
        fun default(cacheDirectory: File): MzClient =
            MzClient(File(cacheDirectory, "musiczero.sock"))

        /** Player snapshot; values follow exactly the mz status JSON. */
        fun parseStatus(reply: JSONObject): Playing {
            return Playing(
                module = reply.optString("module"),
                name = reply.optString("name"),
                playlist = reply.optString("playlist"),
                status = reply.optString("status"),
                trackId = reply.optString("track_id"),
                title = reply.optString("title"),
                artist = reply.optString("artist"),
                positionMs = reply.optLong("position_ms"),
                durationMs = reply.optLong("duration_ms"),
                canSeek = reply.optBoolean("can_seek"),
                canPrevious = reply.optBoolean("can_previous"),
                artUrl = reply.optString("art_url"),
            )
        }

        fun parseHistory(reply: JSONObject): List<Track> {
            val past = reply.optJSONArray("history") ?: return emptyList()
            return past.toTracks()
        }

        fun parsePlaylists(reply: JSONObject): Playlists {
            val items = reply.optJSONArray("playlists") ?: JSONArray()
            return Playlists(
                playlist = reply.optString("playlist"),
                playlists = items.toPlaylists(),
            )
        }
    }
}

private fun JSONArray.toPlaylists(): List<Playlist> {
    val result = mutableListOf<Playlist>()
    for (index in 0 until length()) {
        optJSONObject(index)?.let { element ->
            result.add(Playlist(id = element.optString("id"), name = element.optString("name")))
        }
    }
    return result
}

private fun JSONArray.toTracks(): List<Track> {
    val result = mutableListOf<Track>()
    for (index in 0 until length()) {
        optJSONObject(index)?.let { element ->
            result.add(
                Track(
                    id = element.optString("id"),
                    title = element.optString("title"),
                    artist = element.optString("artist"),
                    artUrl = element.optString("art_url"),
                ),
            )
        }
    }
    return result
}

data class Playing(
    val module: String,
    val name: String,
    val playlist: String,
    val status: String,
    val trackId: String,
    val title: String,
    val artist: String,
    val positionMs: Long,
    val durationMs: Long,
    val canSeek: Boolean,
    val canPrevious: Boolean,
    val artUrl: String,
)

data class Playlist(val id: String, val name: String)

data class Playlists(val playlist: String, val playlists: List<Playlist>)

data class Track(val id: String, val title: String, val artist: String, val artUrl: String)

data class Manifest(
    val id: String,
    val name: String,
    val binary: String,
    val defaultPlaylist: String,
    val json: String = "",
)
