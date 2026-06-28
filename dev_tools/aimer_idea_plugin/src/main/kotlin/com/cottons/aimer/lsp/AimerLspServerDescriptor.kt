package com.cottons.aimer.lsp

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.intellij.platform.lsp.api.ProjectWideLspServerDescriptor
import java.io.File

/**
 * Descriptor for the Aimer LSP server process.
 *
 * Launches the `aimer-lsp` binary via stdio transport.
 */
class AimerLspServerDescriptor(project: Project) : ProjectWideLspServerDescriptor(project, "Aimer") {

    override fun isSupportedFile(file: com.intellij.openapi.vfs.VirtualFile): Boolean {
        return file.extension == "rs"
    }

    override fun createCommandLine(): GeneralCommandLine {
        val lspPath = findLspBinary()
            ?: throw IllegalStateException(
                "aimer-lsp binary not found. Build it with:\n" +
                "  cargo build -p aimer_lsp --release\n" +
                "Or set the AIMER_LSP_PATH environment variable."
            )

        return GeneralCommandLine()
            .withExePath(lspPath)
            .withParameters("--stdio")
            .withWorkDirectory(project.basePath)
            .withParentEnvironmentType(GeneralCommandLine.ParentEnvironmentType.SYSTEM)
    }

    /**
     * Locate the aimer-lsp binary.
     * Priority: AIMER_LSP_PATH env var, then target/release, then target/debug, then PATH.
     */
    private fun findLspBinary(): String? {
        // 1. Environment variable
        val envPath = System.getenv("AIMER_LSP_PATH")
        if (envPath != null && File(envPath).canExecute()) {
            return envPath
        }

        val projectDir = project.basePath ?: return null

        // 2. Project target directories
        val candidates = listOf(
            "$projectDir/target/release/aimer-lsp",
            "$projectDir/target/debug/aimer-lsp",
            "$projectDir/dev_tools/aimer_lsp/target/release/aimer-lsp",
            "$projectDir/dev_tools/aimer_lsp/target/debug/aimer-lsp",
        )

        for (candidate in candidates) {
            if (File(candidate).canExecute()) {
                return candidate
            }
        }

        // 3. PATH
        try {
            val pathResult = ProcessBuilder("which", "aimer-lsp")
                .redirectErrorStream(true)
                .start()
            if (pathResult.waitFor() == 0) {
                return pathResult.inputStream.bufferedReader().readLine()?.trim()
            }
        } catch (_: Exception) {
            // which not available
        }

        return null
    }
}
