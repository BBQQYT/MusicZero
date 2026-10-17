package com.musiczero.app

import android.content.Context
import java.io.File

/**
 * Stages the Rust engine on first run. The build script packs every binary as
 * a jniLib (`libmz.so`, `libymz_module.so`, ...), which Android extracts into
 * the native library directory because useLegacyPackaging is enabled. Here
 * they are copied into filesDir/bin and given their real names, the module
 * manifests are unpacked verbatim from assets next to the module binaries,
 * and the player is then started as a regular child process with the same
 * environment the Termux build expects (HOME, XDG_CONFIG_HOME,
 * XDG_RUNTIME_DIR, TMPDIR, MZ_MODULES_DIR).
 *
 * targetSdk stays at 28 deliberately: newer targets are forbidden from
 * exec()ing anything inside the app data directory, and the engine plus its
 * module binaries must live there.
 */
object Engine {

    fun binDir(context: Context): File = File(context.filesDir, "bin")
    fun modulesDir(context: Context): File = File(context.filesDir, "modules")
    fun configDir(context: Context): File = File(context.filesDir, "config")
    fun socketFile(context: Context): File = File(context.cacheDir, "musiczero.sock")
    fun logFile(context: Context): File = File(context.cacheDir, "mz.log")

    /** jniLib name -> on-disk executable name that mz expects. */
    private val EXECUTABLES = mapOf(
        "libmz.so" to "mz",
        "libymz_module.so" to "ymz-module",
        "libyoumz_module.so" to "youmz-module",
        "libspotify_module.so" to "spotify-module",
        "liblocal_module.so" to "local-module",
        "libicecast_module.so" to "icecast-module",
    )

    /** Returns the manifest of every module staged into the APK assets. */
    fun bundledModules(context: Context): List<Manifest> {
        val result = mutableListOf<Manifest>()
        try {
            context.assets.list("modules")?.sorted()?.forEach { id ->
                context.assets.open("modules/$id/module.json").bufferedReader().use { reader ->
                    val json = reader.readText()
                    result.add(readManifest(id, json))
                }
            }
        } catch (_: Exception) {
            // Assets exist only when scripts/build-android.sh staged them;
            // debug builds without it have no modules and the UI reports that.
        }
        return result
    }

    internal fun readManifest(id: String, json: String): Manifest = try {
        with(org.json.JSONObject(json)) {
            Manifest(
                id = optString("id").ifEmpty { id },
                name = optString("name").ifEmpty { id },
                binary = optString("binary"),
                defaultPlaylist = optString("default_playlist"),
                json = json,
            )
        }
    } catch (_: Exception) {
        Manifest(id = id, name = id, binary = "", defaultPlaylist = "", json = json)
    }

    /** Copies the packaged binaries and manifests into their final layout. */
    fun prepare(context: Context) {
        val bin = binDir(context).apply {
            deleteRecursively()
            mkdirs()
        }
        val nativeLibraryDir = context.applicationInfo.nativeLibraryDir
        if (nativeLibraryDir == null) {
            return
        }
        File(nativeLibraryDir).list()?.forEach { library ->
            val target = EXECUTABLES[library] ?: return@forEach
            copyExecutable(File(nativeLibraryDir, library), File(bin, target))
        }
        val modules = modulesDir(context).apply {
            deleteRecursively()
            mkdirs()
        }
        bundledModules(context).forEach { manifest ->
            val module = File(modules, manifest.id).apply { mkdirs() }
            File(module, "module.json").writeText(manifest.json)
            val launcher = File(bin, manifest.id + "-module")
            if (launcher.exists() && manifest.binary.isNotEmpty()) {
                // The host spawns modules/<id>/<binary from the manifest>.
                copyExecutable(launcher, File(module, manifest.binary))
            }
        }
    }

    private fun copyExecutable(source: File, target: File) {
        try {
            source.inputStream().use { input ->
                target.outputStream().use { output -> input.copyTo(output) }
            }
            target.setExecutable(true, false)
            target.setReadable(true, false)
        } catch (_: Exception) {
            target.delete()
        }
    }

    fun environment(context: Context): Map<String, String> = mapOf(
        "HOME" to context.filesDir.absolutePath,
        "XDG_CONFIG_HOME" to configDir(context).absolutePath,
        "XDG_RUNTIME_DIR" to context.cacheDir.absolutePath,
        "TMPDIR" to context.cacheDir.absolutePath,
        "MZ_MODULES_DIR" to modulesDir(context).absolutePath,
        "RUST_LOG" to "info",
    )

    /** Launches the player process for the given module id. */
    fun start(context: Context, moduleId: String): Boolean {
        val host = File(binDir(context), "mz")
        if (!host.exists()) {
            return false
        }
        // Several pollers (app screen, service) may race the startup window;
        // only one spawn request every few seconds goes through.
        synchronized(LOCK) {
            val now = System.currentTimeMillis()
            if (now - LAST_SPAWN < 4_000) {
                return true
            }
            LAST_SPAWN = now
        }
        val log = logFile(context)
        if (log.length() > 4 * 1024 * 1024) {
            log.delete()
        }
        return try {
            val builder = ProcessBuilder(host.absolutePath, "start", moduleId)
                .directory(context.cacheDir)
                .redirectErrorStream(true)
                .redirectOutput(ProcessBuilder.Redirect.appendTo(log))
            builder.environment().putAll(environment(context))
            PROCESS = builder.start()
            true
        } catch (_: Exception) {
            false
        }
    }

    private val LOCK = Any()

    @Volatile
    private var LAST_SPAWN = 0L

    fun stop() {
        try {
            PROCESS?.destroy()
        } catch (_: Exception) {
        }
        PROCESS = null
    }

    private var PROCESS: Process? = null
}
