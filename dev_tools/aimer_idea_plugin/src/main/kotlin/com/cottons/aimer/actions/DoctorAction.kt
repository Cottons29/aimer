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
 * Tools > Aimer > Doctor — Check that required toolchains are installed.
 */
class DoctorAction : AnAction() {

    override fun actionPerformed(e: AnActionEvent) {
        val project = e.project ?: return

        object : Task.Backgroundable(project, "Running Aimer Doctor...", true) {
            override fun run(indicator: ProgressIndicator) {
                try {
                    val process = ProcessBuilder("aimer", "doctor")
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
                                    "Aimer Doctor",
                                    output.take(500),
                                    NotificationType.INFORMATION
                                )
                                .notify(project)
                        } else {
                            Messages.showWarningDialog(
                                project,
                                "Doctor check found issues:\n\n${output.takeLast(500)}",
                                "Aimer Doctor"
                            )
                        }
                    }
                } catch (ex: Exception) {
                    com.intellij.openapi.application.ApplicationManager.getApplication().invokeLater {
                        Messages.showErrorDialog(
                            project,
                            "Failed to run aimer doctor: ${ex.message}",
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
