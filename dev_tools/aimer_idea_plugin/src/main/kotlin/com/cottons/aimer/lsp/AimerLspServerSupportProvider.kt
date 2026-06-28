package com.cottons.aimer.lsp

import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.platform.lsp.api.LspServerSupportProvider

/**
 * Registers the Aimer LSP server with the IntelliJ platform.
 *
 * The LSP server activates when the project contains an Aimer.toml file
 * at the project root, indicating it's an Aimer project.
 */
class AimerLspServerSupportProvider : LspServerSupportProvider {

    override fun fileOpened(
        project: Project,
        file: VirtualFile,
        serverStarter: LspServerSupportProvider.LspServerStarter
    ) {
        // Only activate for Rust files in Aimer projects
        if (file.extension != "rs") return

        val basePath = project.basePath ?: return
        val aimerToml = java.io.File(basePath, "Aimer.toml")
        if (!aimerToml.exists()) return

        // Start the LSP server
        try {
            val descriptor = AimerLspServerDescriptor(project)
            // Use reflection-free approach: the LspServerStarter interface
            // accepts a descriptor to ensure the server is running
            serverStarter.ensureServerStarted(descriptor)
        } catch (_: Exception) {
            // LSP binary not found — this is expected if user hasn't built it
        }
    }
}
