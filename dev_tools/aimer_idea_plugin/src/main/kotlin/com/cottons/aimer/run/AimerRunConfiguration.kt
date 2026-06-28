package com.cottons.aimer.run

import com.intellij.execution.Executor
import com.intellij.execution.configurations.*
import com.intellij.execution.runners.ExecutionEnvironment
import com.intellij.openapi.options.SettingsEditor
import com.intellij.openapi.project.Project
import org.jdom.Element

/**
 * Run configuration for Aimer CLI commands.
 *
 * Stores the target platform, release mode, and optional device name.
 */
class AimerRunConfiguration(
    project: Project,
    factory: ConfigurationFactory,
    name: String
) : RunConfigurationBase<Element>(project, factory, name) {

    /** The target platform (e.g., "macos", "ios", "web", "android"). */
    var target: String = "macos"

    /** Whether to build in release mode. */
    var release: Boolean = false

    /** Optional device name for `aimer run --device`. */
    var device: String = ""

    /** The CLI command to execute: "run", "assemble", or "doctor". */
    var command: String = "run"

    override fun getConfigurationEditor(): SettingsEditor<out RunConfiguration> {
        return AimerRunConfigEditor()
    }

    override fun checkConfiguration() {
        if (target.isBlank() && command != "doctor") {
            throw RuntimeConfigurationError("Target platform must be specified")
        }
    }

    override fun getState(executor: Executor, environment: ExecutionEnvironment): RunProfileState {
        return AimerRunProfileState(environment, this)
    }

    override fun readExternal(element: Element) {
        super.readExternal(element)
        target = element.getAttributeValue("target", "macos")
        release = element.getAttributeValue("release", "false").toBoolean()
        device = element.getAttributeValue("device", "")
        command = element.getAttributeValue("command", "run")
    }

    override fun writeExternal(element: Element) {
        super.writeExternal(element)
        element.setAttribute("target", target)
        element.setAttribute("release", release.toString())
        element.setAttribute("device", device)
        element.setAttribute("command", command)
    }
}
