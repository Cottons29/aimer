package com.cottons.aimer.widgetTree

import com.intellij.openapi.components.Service
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.psi.search.FilenameIndex
import com.intellij.psi.search.GlobalSearchScope

/**
 * Service that discovers Aimer widgets in the project.
 *
 * Uses regex-based scanning of Rust source files as a fallback when the LSP
 * is not available. When the LSP is connected, the tool window will prefer
 * the LSP-provided widget tree data.
 */
@Service(Service.Level.PROJECT)
class WidgetTreeService(private val project: Project) {

    companion object {
        private val WIDGET_ATTR_REGEX = Regex("""#\[widget\((\w+)\)\]""")
        private val ENTRY_POINT_ATTR_REGEX = Regex("""#\[(?:aimer::)?main]""")
        private val STRUCT_REGEX = Regex("""(?:pub\s+)?(?:struct|enum)\s+(\w+)""")
        private val FN_REGEX = Regex("""(?:pub\s+)?(?:async\s+)?fn\s+(\w+)""")
    }

    /**
     * Scan all Rust files in the project for #[widget] declarations.
     */
    fun scanProject(): List<WidgetTreeNode> {
        val nodes = mutableListOf<WidgetTreeNode>()

        val scope = GlobalSearchScope.projectScope(project)
        val rustFiles = FilenameIndex.getAllFilesByExt(project, "rs", scope)

        for (file in rustFiles) {
            val fileNodes = scanFile(file)
            nodes.addAll(fileNodes)
        }

        return nodes
    }

    /**
     * Scan a single Rust file for #[widget] and entry point declarations.
     */
    fun scanFile(file: VirtualFile): List<WidgetTreeNode> {
        val nodes = mutableListOf<WidgetTreeNode>()

        try {
            val content = String(file.contentsToByteArray())
            val lines = content.lines()

            var i = 0
            while (i < lines.size) {
                val line = lines[i].trim()

                // Check for #[widget(...)] attribute
                val widgetMatch = WIDGET_ATTR_REGEX.find(line)
                if (widgetMatch != null) {
                    val kindStr = widgetMatch.groupValues[1]
                    val kind = WidgetKind.fromString(kindStr)

                    // Find the struct/enum declaration on the next lines
                    for (j in (i + 1)..minOf(i + 5, lines.size - 1)) {
                        val declLine = lines[j].trim()
                        if (declLine.isEmpty() || declLine.startsWith("#") || declLine.startsWith("//")) {
                            continue
                        }

                        val structMatch = STRUCT_REGEX.find(declLine)
                        if (structMatch != null) {
                            val name = structMatch.groupValues[1]
                            nodes.add(
                                WidgetTreeNode(
                                    name = name,
                                    kind = kind,
                                    fileUri = file.url,
                                    line = i
                                )
                            )
                            break
                        }
                    }
                }

                // Check for #[aimer::main] or #[main] entry point
                if (ENTRY_POINT_ATTR_REGEX.matches(line)) {
                    // Find the fn declaration on the next lines
                    for (j in (i + 1)..minOf(i + 5, lines.size - 1)) {
                        val declLine = lines[j].trim()
                        if (declLine.isEmpty() || declLine.startsWith("#") || declLine.startsWith("//")) {
                            continue
                        }

                        val fnMatch = FN_REGEX.find(declLine)
                        if (fnMatch != null) {
                            val name = fnMatch.groupValues[1]
                            nodes.add(
                                WidgetTreeNode(
                                    name = name,
                                    kind = WidgetKind.ENTRY_POINT,
                                    fileUri = file.url,
                                    line = i
                                )
                            )
                            break
                        }
                    }
                }

                i++
            }
        } catch (_: Exception) {
            // Silently skip files that can't be read
        }

        return nodes
    }
}
