package com.cottons.aimer.actions

import com.intellij.codeInsight.daemon.LineMarkerInfo
import com.intellij.codeInsight.daemon.LineMarkerProvider
import com.intellij.icons.AllIcons
import com.intellij.openapi.editor.markup.GutterIconRenderer
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.Task
import com.intellij.openapi.ui.Messages
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.psi.PsiElement

/**
 * Shows a green "Run" gutter icon next to functions annotated with
 * `#[aimer::main]` or `#[main]` in Rust files.
 *
 * Clicking the icon runs `aimer run` for the project.
 */
class AimerMainRunLineMarkerContributor : LineMarkerProvider {

    override fun getLineMarkerInfo(element: PsiElement): LineMarkerInfo<*>? {
        if (!isAimerMainAttribute(element)) return null

        val project = element.project
        val virtualFile = element.containingFile?.virtualFile ?: return null
        if (virtualFile.extension != "rs") return null

        return LineMarkerInfo(
            element,
            element.textRange,
            AllIcons.Actions.Execute,
            { "Run Aimer App" },
            { _, _ -> showTargetPicker(project) },
            GutterIconRenderer.Alignment.LEFT
        )
    }

    /**
     * Check if this PSI element is part of an `#[aimer::main]` or `#[main]` attribute.
     */
    private fun isAimerMainAttribute(element: PsiElement): Boolean {
        val text = element.text

        // Fast reject
        if (text != "main") return false

        // Check the parent chain for attribute context.
        // For `#[aimer::main]`, the PSI tree is roughly:
        //   RsAttr -> RsPath -> RsPathSegment("aimer") + RsPathSegment("main")
        // For `#[main]`:
        //   RsAttr -> RsPath -> RsPathSegment("main")
        val parent = element.parent ?: return false

        // Match `aimer::main` path
        if (parent.text == "aimer::main") return true

        // Match standalone `main` (not part of a longer path)
        if (parent.text == "main") return true

        return false
    }

    private fun showTargetPicker(project: com.intellij.openapi.project.Project) {
        val targets = arrayOf(
            "macos", "ios", "ios-simulator",
            "android", "android-simulator",
            "web", "windows", "linux"
        )

        val target = Messages.showChooseDialog(
            project,
            "Select target platform:",
            "Run Aimer App",
            Messages.getQuestionIcon(),
            targets,
            "macos"
        )
        if (target < 0) return

        val selectedTarget = targets[target]

        object : Task.Backgroundable(project, "Running Aimer ($selectedTarget)...", true) {
            override fun run(indicator: ProgressIndicator) {
                try {
                    val process = ProcessBuilder("aimer", "run", "--target", selectedTarget, "--no-tui")
                        .directory(java.io.File(project.basePath ?: "."))
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
}
