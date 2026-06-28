package com.cottons.aimer.widgetTree

import com.intellij.openapi.fileEditor.OpenFileDescriptor
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFileManager
import com.intellij.ui.components.JBLabel
import com.intellij.ui.components.JBPanel
import com.intellij.ui.components.JBScrollPane
import com.intellij.ui.treeStructure.Tree
import java.awt.BorderLayout
import java.awt.event.MouseAdapter
import java.awt.event.MouseEvent
import javax.swing.tree.DefaultMutableTreeNode
import javax.swing.tree.DefaultTreeModel

/**
 * Panel that displays the Aimer widget tree.
 */
class WidgetTreePanel(private val project: Project) : JBPanel<JBPanel<*>>() {

    private val treeModel = DefaultTreeModel(DefaultMutableTreeNode("Aimer Widgets"))
    private val tree = Tree(treeModel)
    private val statusLabel = JBLabel("")

    init {
        layout = BorderLayout()

        tree.isRootVisible = true
        tree.showsRootHandles = true

        // Double-click to navigate to source
        tree.addMouseListener(object : MouseAdapter() {
            override fun mouseClicked(e: MouseEvent) {
                if (e.clickCount == 2) {
                    val node = tree.lastSelectedPathComponent as? DefaultMutableTreeNode ?: return
                    val userObject = node.userObject
                    if (userObject is WidgetTreeNode) {
                        navigateToSource(userObject)
                    }
                }
            }
        })

        val scrollPane = JBScrollPane(tree)
        add(scrollPane, BorderLayout.CENTER)
        add(statusLabel, BorderLayout.SOUTH)

        refresh()
    }

    /**
     * Refresh the tree by scanning the project.
     */
    fun refresh() {
        statusLabel.text = "Scanning..."

        val service = project.getService(WidgetTreeService::class.java)
        val widgets = service.scanProject()

        val root = DefaultMutableTreeNode("Aimer Widgets")

        // Group by kind
        val grouped = widgets.groupBy { it.kind }
        for ((kind, kindWidgets) in grouped) {
            val kindNode = DefaultMutableTreeNode(kind.label)
            for (widget in kindWidgets) {
                val widgetNode = DefaultMutableTreeNode(widget)
                kindNode.add(widgetNode)
            }
            root.add(kindNode)
        }

        treeModel.setRoot(root)
        tree.expandRow(0)

        statusLabel.text = if (widgets.isEmpty()) {
            "No Aimer widgets found"
        } else {
            "${widgets.size} widget(s) found"
        }
    }

    private fun navigateToSource(widget: WidgetTreeNode) {
        val url = widget.fileUri
        val virtualFile = VirtualFileManager.getInstance().findFileByUrl(url) ?: return
        OpenFileDescriptor(project, virtualFile, widget.line, 0).navigate(true)
    }
}
