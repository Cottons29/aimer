package com.cottons.aimer.actions

import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.ui.Messages
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.Task
import java.io.File

/**
 * Tools > Aimer > Run — Quick-run an Aimer app with a target picker dialog.
 */
class RunAction : AnAction() {

    companion object {
        private val TARGETS = arrayOf(
            "macos", "ios", "ios-simulator",
            "android", "android-simulator",
            "web", "windows", "linux"
        )
    }

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return

        val target = Messages.showChooseDialog(
            project,
            "Select target platform:",
            "Run Aimer App",
            Messages.getQuestionIcon(),
            TARGETS,
            "macos"
        )
        if (target < 0) return

        val selectedTarget = TARGETS[target]

        object : Task.Backgroundable(project, "Running Aimer ($selectedTarget)...", true) {
            override fun run(indicator: ProgressIndicator) {
                try {
                    val process = ProcessBuilder("aimer", "run", "--target", selectedTarget, "--no-tui")
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
                                    "App launched successfully on $selectedTarget",
                                    NotificationType.INFORMATION
                                )
                                .notify(project)
                        } else {
                            Messages.showErrorDialog(
                                project,
                                "Aimer run failed (exit code $exitCode):\n\n${output.takeLast(500)}",
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
