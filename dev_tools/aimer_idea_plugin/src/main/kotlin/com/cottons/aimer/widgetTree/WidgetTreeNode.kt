package com.cottons.aimer.widgetTree

import com.intellij.icons.AllIcons
import javax.swing.Icon

/**
 * Represents a single widget in the Aimer widget tree.
 */
data class WidgetTreeNode(
    val name: String,
    val kind: WidgetKind,
    val fileUri: String,
    val line: Int,
    val children: MutableList<WidgetTreeNode> = mutableListOf()
) {
    fun getIcon(): Icon = when (kind) {
        WidgetKind.STATELESS -> AllIcons.Nodes.Class
        WidgetKind.STATEFUL -> AllIcons.Nodes.AbstractClass
        WidgetKind.ROUTER -> AllIcons.Nodes.Enum
        WidgetKind.RAW_WIDGET -> AllIcons.Nodes.Interface
        WidgetKind.ENTRY_POINT -> AllIcons.Nodes.Method
    }

    override fun toString(): String = "$name (${kind.label})"
}

enum class WidgetKind(val label: String) {
    STATELESS("Stateless"),
    STATEFUL("Stateful"),
    ROUTER("Router"),
    RAW_WIDGET("RawWidget"),
    ENTRY_POINT("EntryPoint");

    companion object {
        fun fromString(s: String): WidgetKind = when (s.lowercase()) {
            "stateless" -> STATELESS
            "stateful" -> STATEFUL
            "router" -> ROUTER
            "rawwidget", "raw_widget" -> RAW_WIDGET
            "entrypoint", "entry_point" -> ENTRY_POINT
            else -> STATELESS
        }
    }
}
