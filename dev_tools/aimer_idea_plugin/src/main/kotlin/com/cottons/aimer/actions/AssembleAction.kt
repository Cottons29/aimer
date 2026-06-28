package com.cottons.aimer.actions

import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.Task
import com.intellij.openapi.ui.Messages
import java.io.File

/**
 * Tools > Aimer > Assemble — Assemble a distributable bundle.
 */
class AssembleAction : AnAction() {

    companion object {
        private val PLATFORMS = arrayOf(
            "macos", "ios", "ios-simulator",
            "android", "android-simulator",
            "web", "windows", "linux"
        )
    }

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return

        val platform = Messages.showChooseDialog(
            project,
            "Select platform to assemble:",
            "Assemble Aimer Bundle",
            Messages.getQuestionIcon(),
            PLATFORMS,
            "macos"
        )
        if (platform < 0) return

        val selectedPlatform = PLATFORMS[platform]

        val release = Messages.showYesNoDialog(
            project,
            "Build in release mode?",
            "Aimer Assemble",
            "Release",
            "Debug",
            Messages.getQuestionIcon()
        ) == Messages.YES

        val args = mutableListOf("aimer", "assemble", selectedPlatform)
        if (release) args.add("--release")

        object : Task.Backgroundable(project, "Assembling $selectedPlatform...", true) {
            override fun run(indicator: ProgressIndicator) {
                try {
                    val process = ProcessBuilder(args)
                        .directory(File(project.basePath ?: "."))
                        .redirectErrorStream(true)
                        .start()

                    val output = process.inputStream.bufferedReader().readText()
                    val exitCode = process.waitFor()

                    com.intellij.openapi.application.ApplicationManager.getApplication().invokeLater {
                        if (exitCode == 0) {
                            NotificationGroupManager.getInstance()
                                .getNotificationGroup("Aimer")
                                .createNotification(
                                    "Aimer",
                                    "Bundle assembled successfully for $selectedPlatform",
                                    NotificationType.INFORMATION
                                )
                                .notify(project)
                        } else {
                            Messages.showErrorDialog(
                                project,
                                "Assemble failed (exit code $exitCode):\n\n${output.takeLast(500)}",
                                "Aimer Error"
                            )
                        }
                    }
                } catch (ex: Exception) {
                    com.intellij.openapi.application.ApplicationManager.getApplication().invokeLater {
                        Messages.showErrorDialog(
                            project,
                            "Failed to run aimer: ${ex.message}",
                            "Aimer Error"
                        )
                    }
                }
            }
        }.queue()
    }

    override fun update(e: AnActionEvent) {
        e.presentation.isEnabled = e.project != null
    }
}
