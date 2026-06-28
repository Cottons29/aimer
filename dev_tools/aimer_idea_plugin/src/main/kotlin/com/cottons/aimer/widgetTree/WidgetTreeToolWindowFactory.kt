package com.cottons.aimer.widgetTree

import com.intellij.openapi.actionSystem.ActionManager
import com.intellij.openapi.actionSystem.ActionToolbar
import com.intellij.openapi.actionSystem.DefaultActionGroup
import com.intellij.openapi.project.Project
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.content.ContentFactory

/**
 * Factory for the "Aimer Widget Tree" tool window.
 */
class WidgetTreeToolWindowFactory : ToolWindowFactory {

    override fun shouldBeAvailable(project: Project) = true

    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val panel = WidgetTreePanel(project)

        // Add refresh action to the toolbar
        val actionGroup = DefaultActionGroup()
        val refreshAction = RefreshWidgetTreeAction(panel)
        actionGroup.add(refreshAction)

        val toolbar: ActionToolbar = ActionManager.getInstance()
            .createActionToolbar("AimerWidgetTree", actionGroup, true)
        toolbar.targetComponent = panel

        val contentPanel = com.intellij.ui.components.JBPanel<com.intellij.ui.components.JBPanel<*>>().apply {
            layout = java.awt.BorderLayout()
            add(toolbar.component, java.awt.BorderLayout.NORTH)
            add(panel, java.awt.BorderLayout.CENTER)
        }

        val content = ContentFactory.getInstance().createContent(contentPanel, null, false)
        toolWindow.contentManager.addContent(content)
    }
}

/**
 * Action to refresh the widget tree.
 */
private class RefreshWidgetTreeAction(private val panel: WidgetTreePanel) :
    com.intellij.openapi.actionSystem.AnAction(
        "Refresh",
        "Rescan project for Aimer widgets",
        com.intellij.icons.AllIcons.Actions.Refresh
    ) {
    override fun actionPerformed(e: com.intellij.openapi.actionSystem.AnActionEvent) {
        panel.refresh()
    }
}
