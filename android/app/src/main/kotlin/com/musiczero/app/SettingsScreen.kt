@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)

package com.musiczero.app

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.json.JSONArray
import org.json.JSONObject

/**
 * Engine controls, the active provider's settings (a generic form over the
 * module's `settings` JSON) and the app info block.
 */
@Composable
fun SettingsScreen(state: PlayerState, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    var tokenSheet by remember { mutableStateOf(false) }

    LaunchedEffect(state.status?.module, state.connection) {
        if (state.connection == Connection.Up) {
            state.loadSettings()
        }
    }

    Column(
        modifier = modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(bottom = 22.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Text(
            text = stringResource(R.string.tab_settings),
            style = MaterialTheme.typography.headlineSmall,
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 14.dp),
        )
        EngineCard(state, modifier = Modifier.padding(horizontal = 20.dp))
        ProviderCard(
            state = state,
            onToken = { tokenSheet = true },
            modifier = Modifier.padding(horizontal = 20.dp),
        )
        AboutCard(modifier = Modifier.padding(horizontal = 20.dp))
    }

    if (tokenSheet) {
        ModalBottomSheet(onDismissRequest = { tokenSheet = false }) {
            TokenSheet(context) {
                tokenSheet = false
                state.loadSettings()
            }
        }
    }
}

@Composable
private fun EngineCard(state: PlayerState, modifier: Modifier = Modifier) {
    val stateText = when {
        state.suspended -> stringResource(R.string.engine_state_down)
        state.connection == Connection.Up -> stringResource(R.string.engine_state_running)
        state.connection == Connection.Starting -> stringResource(R.string.engine_state_starting)
        else -> stringResource(R.string.engine_state_down)
    }
    SettingsCard(title = stringResource(R.string.settings_engine), modifier = modifier) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            StatusDot(state.connection)
            Spacer(Modifier.width(10.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(stateText, style = MaterialTheme.typography.titleMedium)
                val moduleName = state.status?.name
                if (!moduleName.isNullOrEmpty()) {
                    Text(
                        text = moduleName,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            if (state.suspended || state.connection == Connection.Down) {
                FilledTonalButton(onClick = { state.startEngine() }) {
                    Text(stringResource(R.string.settings_start))
                }
            } else {
                FilledTonalButton(onClick = { state.stopEngine() }) {
                    Text(stringResource(R.string.settings_stop))
                }
            }
        }
    }
}

@Composable
private fun ProviderCard(
    state: PlayerState,
    onToken: () -> Unit,
    modifier: Modifier = Modifier,
) {
    SettingsCard(title = stringResource(R.string.settings_provider), modifier = modifier) {
        val settings = state.settings
        when {
            state.status == null -> Text(
                text = stringResource(R.string.engine_down),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            state.settingsError != null -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(
                    text = state.settingsError.orEmpty(),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                )
                TextButton(onClick = { state.loadSettings() }) {
                    Text(stringResource(R.string.settings_reload))
                }
            }
            settings == null -> Text(
                text = stringResource(R.string.settings_loading),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            else -> SettingsBody(state, settings, onToken)
        }
    }
}

@Composable
private fun ColumnScope.SettingsBody(
    state: PlayerState,
    settings: JSONObject,
    onToken: () -> Unit,
) {
    val moduleId = state.status?.module.orEmpty()
    settings.optString("warning").takeIf { it.isNotEmpty() }?.let { warning ->
        Text(
            text = warning,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    if (moduleId == "ymz") {
        TokenRow(state, onToken)
    }
    val keys = settings.keys().asSequence()
        .filter { key -> key != "warning" && !(moduleId == "ymz" && key == "token") }
        .sorted()
        .toList()
    var shown = false
    keys.forEach { key ->
        val value = settings.opt(key)
        if (value == null || value == JSONObject.NULL) {
            return@forEach
        }
        shown = true
        SettingField(key = key, value = value) { text -> state.setSetting(key, text) }
    }
    if (!shown && moduleId != "ymz") {
        Text(
            text = stringResource(R.string.provider_no_settings),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun TokenRow(state: PlayerState, onEdit: () -> Unit) {
    val token = state.settings?.optString("token").orEmpty()
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = stringResource(R.string.token_title),
                style = MaterialTheme.typography.bodyLarge,
            )
            Text(
                text = stringResource(if (token.isEmpty()) R.string.token_unset else R.string.token_set),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        FilledTonalButton(onClick = onEdit) {
            Text(stringResource(R.string.token_edit))
        }
    }
}

@Composable
private fun SettingField(key: String, value: Any, onSave: (String) -> Unit) {
    val label = remember(key) {
        key.replace('_', ' ').replaceFirstChar { it.uppercase() }
    }
    when (value) {
        is Boolean -> Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = label,
                style = MaterialTheme.typography.bodyLarge,
                modifier = Modifier.weight(1f),
            )
            Switch(checked = value, onCheckedChange = { onSave(it.toString()) })
        }
        is String -> EditableSetting(label, value, numeric = false, onSave)
        is Number -> EditableSetting(label, value.toString(), numeric = true, onSave)
        is JSONObject, is JSONArray -> Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(label, style = MaterialTheme.typography.bodyLarge)
            Text(
                text = value.toString(),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = stringResource(R.string.setting_complex),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

@Composable
private fun EditableSetting(
    label: String,
    value: String,
    numeric: Boolean,
    onSave: (String) -> Unit,
) {
    var text by remember(value) { mutableStateOf(value) }
    OutlinedTextField(
        value = text,
        onValueChange = { text = it },
        label = { Text(label) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(
            keyboardType = if (numeric) KeyboardType.Number else KeyboardType.Text,
        ),
        trailingIcon = if (text != value) {
            {
                TextButton(onClick = { onSave(text.trim()) }) {
                    Text(stringResource(R.string.save))
                }
            }
        } else {
            null
        },
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun AboutCard(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val version = remember {
        runCatching {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName
        }.getOrNull() ?: "?"
    }
    SettingsCard(title = stringResource(R.string.settings_about), modifier = modifier) {
        Text(
            text = stringResource(R.string.settings_version, version),
            style = MaterialTheme.typography.bodyLarge,
        )
        Text(
            text = stringResource(R.string.settings_about_text),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun SettingsCard(
    title: String,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(24.dp),
        color = MaterialTheme.colorScheme.surfaceContainer,
    ) {
        Column(
            modifier = Modifier.padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            content()
        }
    }
}

@Composable
private fun TokenSheet(context: Context, onSaved: () -> Unit) {
    var token by remember { mutableStateOf(readToken(context)) }
    Column(
        modifier = Modifier
            .padding(24.dp)
            .padding(bottom = 12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(stringResource(R.string.token_title), style = MaterialTheme.typography.titleLarge)
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
                    onSaved()
                },
            ) { Text(stringResource(R.string.save)) }
            TextButton(
                modifier = Modifier.weight(1f),
                onClick = onSaved,
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
