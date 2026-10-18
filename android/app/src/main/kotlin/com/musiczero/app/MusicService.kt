package com.musiczero.app

import android.app.PendingIntent
import android.content.Intent
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaLibraryService
import androidx.media3.session.MediaSession
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/**
 * Owns the media session: the Android Auto browser, the lock screen and the
 * notification all mirror this state, while the real playback happens inside
 * the mz engine process spawned from the same private directory.
 */
class MusicService : MediaLibraryService() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var session: MediaLibrarySession? = null
    private var player: EnginePlayer? = null
    private var modules: List<Manifest> = emptyList()
    private var playlists = Playlists("", emptyList())
    private var listedFor: Pair<String, String>? = null
    private var startedAt = 0L

    override fun onCreate() {
        super.onCreate()
        Engine.prepare(this)
        modules = Engine.bundledModules(this)
        player = EnginePlayer { request ->
            scope.launch {
                MzClient.default(cacheDir).call(request)
            }
        }
        session = MediaLibrarySession.Builder(this, player!!, Bridge())
            .setSessionActivity(
                PendingIntent.getActivity(
                    this,
                    0,
                    Intent(this, MainActivity::class.java),
                    PendingIntent.FLAG_IMMUTABLE,
                ),
            )
            .build()
        scope.launch { polling() }
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaLibrarySession? {
        return session
    }

    private suspend fun polling() {
        while (true) {
            val playing = readStatus()
            withContext(Dispatchers.Main.immediate) {
                player?.update(playing)
            }
            if (playing == null) {
                maybeStartEngine()
            }
            delay(1000)
        }
    }

    private suspend fun readStatus(): Playing? = withContext(Dispatchers.IO) {
        val client = MzClient.default(cacheDir)
        val reply = client.call(JSONObject().put("action", "status")) ?: return@withContext null
        val status = MzClient.parseStatus(reply)
        val key = status.module to status.playlist
        if (listedFor != key) {
            client.call(JSONObject().put("action", "playlists"))?.let { listing ->
                playlists = MzClient.parsePlaylists(listing)
                listedFor = key
                session?.notifyChildrenChanged(BROWSE_ROOT, playlists.playlists.size, null)
            }
        }
        status
    }

    /** Same engine respawn rules the app screen follows. */
    private suspend fun maybeStartEngine() {
        if (modules.isEmpty() || Engine.isSuspended(this)) {
            return
        }
        val now = System.currentTimeMillis()
        if (startedAt != 0L && now - startedAt < 6_000) {
            return
        }
        startedAt = now
        withContext(Dispatchers.IO) {
            Engine.start(this@MusicService, Engine.chosenModule(this@MusicService, modules))
        }
    }

    override fun onDestroy() {
        // Swipe of the notification (or task) must not leave audio behind.
        scope.launch {
            MzClient.default(cacheDir).call(JSONObject().put("action", "quit"))
            Engine.stop()
        }
        scope.coroutineContext.cancelChildren()
        session?.release()
        session = null
        player = null
        super.onDestroy()
    }

    private companion object {
        const val BROWSE_ROOT = "playlists"
    }

    /**
     * The browse tree shown in Android Auto and any other MediaBrowser: the
     * current module's playlists are playable entries; launching one asks the
     * engine to switch to it.
     */
    private inner class Bridge : MediaLibraryService.MediaLibrarySession.Callback {

        override fun onGetLibraryRoot(
            session: MediaLibraryService.MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            params: MediaLibraryService.LibraryParams?,
        ): ListenableFuture<LibraryResult<MediaItem>> =
            Futures.immediateFuture(LibraryResult.ofItem(MediaItem.EMPTY, params))

        override fun onGetChildren(
            session: MediaLibraryService.MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            parentId: String,
            page: Int,
            pageSize: Int,
            params: MediaLibraryService.LibraryParams?,
        ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> {
            if (parentId != BROWSE_ROOT) {
                return Futures.immediateFuture(LibraryResult.ofError(LibraryResult.RESULT_ERROR_BAD_VALUE))
            }
            val children = playlists.playlists.map { entry ->
                MediaItem.Builder()
                    .setMediaId("playlist:" + entry.id)
                    .setMediaMetadata(
                        MediaMetadata.Builder()
                            .setTitle(entry.name)
                            .setIsBrowsable(false)
                            .setIsPlayable(true)
                            .build(),
                    )
                    .build()
            }
            return Futures.immediateFuture(LibraryResult.ofItemList(children, params))
        }

        override fun onGetItem(
            session: MediaLibraryService.MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            mediaId: String,
        ): ListenableFuture<LibraryResult<MediaItem>> {
            val entry = playlists.playlists.firstOrNull { "playlist:" + it.id == mediaId }
            if (entry == null) {
                return Futures.immediateFuture(LibraryResult.ofError(LibraryResult.RESULT_ERROR_BAD_VALUE))
            }
            val item = MediaItem.Builder()
                .setMediaId("playlist:" + entry.id)
                .setMediaMetadata(
                    MediaMetadata.Builder()
                        .setTitle(entry.name)
                        .setIsBrowsable(false)
                        .setIsPlayable(true)
                        .build(),
                )
                .build()
            return Futures.immediateFuture(LibraryResult.ofItem(item, null))
        }

        override fun onAddMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            requests: List<MediaItem>,
        ): ListenableFuture<List<MediaItem>> {
            // Choose a playlist from Auto: ask the engine to play it and hand
            // back the node so the session can enqueue it.
            val first = requests.firstOrNull() ?: return Futures.immediateFuture(emptyList())
            val requested = first.mediaId.removePrefix("playlist:")
            val entry = playlists.playlists.firstOrNull { it.id == requested }
            if (entry == null) {
                return Futures.immediateFuture(emptyList())
            }
            scope.launch {
                MzClient.default(cacheDir).call(
                    JSONObject().apply {
                        put("action", "playlist")
                        put("value", entry.id)
                    },
                )
            }
            return Futures.immediateFuture(listOf(first))
        }
    }
}
