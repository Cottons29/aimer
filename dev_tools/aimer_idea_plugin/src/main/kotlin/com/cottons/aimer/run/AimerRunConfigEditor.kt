package com.cottons.aimer.run

import com.intellij.openapi.options.SettingsEditor
import com.intellij.ui.dsl.builder.panel
import javax.swing.DefaultComboBoxModel
import javax.swing.JCheckBox
import javax.swing.JComboBox
import javax.swing.JComponent
import javax.swing.JTextField

/**
 * Settings editor for [AimerRunConfiguration].
 *
 * Provides UI for selecting target platform, release mode, command, and device.
 */
class AimerRunConfigEditor : SettingsEditor<AimerRunConfiguration>() {

    companion object {
        val TARGETS = arrayOf(
            "macos", "ios", "ios-simulator",
            "android", "android-simulator",
            "web", "windows", "linux"
        )
        val COMMANDS = arrayOf("run", "assemble", "build")
    }

    private val commandCombo = JComboBox(COMMANDS)
    private val targetCombo = JComboBox(TARGETS)
    private val releaseCheck = JCheckBox("Release mode")
    private val deviceField = JTextField()

    override fun resetEditorFrom(config: AimerRunConfiguration) {
        commandCombo.selectedItem = config.command
        targetCombo.selectedItem = config.target
        releaseCheck.isSelected = config.release
        deviceField.text = config.device
    }

    override fun applyEditorTo(config: AimerRunConfiguration) {
        config.command = commandCombo.selectedItem as String? ?: "run"
        config.target = targetCombo.selectedItem as String? ?: "macos"
        config.release = releaseCheck.isSelected
        config.device = deviceField.text
    }

    override fun createEditor(): JComponent {
        return panel {
            row("Command:") {
                cell(commandCombo)
            }
            row("Target:") {
                cell(targetCombo)
            }
            row {
                cell(releaseCheck)
            }
            row("Device:") {
                cell(deviceField)
                    .comment("Optional device name for aimer run --device")
            }
        }
    }
}
