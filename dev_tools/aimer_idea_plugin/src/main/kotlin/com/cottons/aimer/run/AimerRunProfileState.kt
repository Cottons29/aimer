package com.cottons.aimer.run

import com.intellij.execution.DefaultExecutionResult
import com.intellij.execution.ExecutionException
import com.intellij.execution.ExecutionResult
import com.intellij.execution.Executor
import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.execution.configurations.RunProfileState
import com.intellij.execution.process.KillableProcessHandler
import com.intellij.execution.runners.ExecutionEnvironment
import com.intellij.execution.runners.ProgramRunner
import com.intellij.execution.impl.ConsoleViewImpl
import java.io.File

/**
 * Execution state for [AimerRunConfiguration].
 *
 * Builds the CLI command line and spawns the aimer process.
 */
class AimerRunProfileState(
    private val environment: ExecutionEnvironment,
    private val config: AimerRunConfiguration
) : RunProfileState {

    override fun execute(executor: Executor, runner: ProgramRunner<*>): ExecutionResult {
        val commandLine = buildCommandLine()
        val handler = KillableProcessHandler(commandLine)

        handler.startNotify()

        val consoleView = ConsoleViewImpl(environment.project, true)
        consoleView.attachToProcess(handler)

        return DefaultExecutionResult(consoleView, handler)
    }

    private fun buildCommandLine(): GeneralCommandLine {
        val aimerPath = findAimerCli()
            ?: throw ExecutionException(
                "aimer CLI not found. Install it or add it to PATH.\n" +
                "Run: cargo install --path dev_tools/aimer_cli"
            )

        val args = mutableListOf<String>()

        when (config.command) {
            "run" -> {
                args.add("run")
                if (config.target.isNotBlank()) {
                    args.addAll(listOf("--target", config.target))
                }
                if (config.device.isNotBlank()) {
                    args.addAll(listOf("--device", config.device))
                }
                args.add("--no-tui")
            }
            "assemble" -> {
                args.add("assemble")
                args.add(config.target)
                if (config.release) {
                    args.add("--release")
                }
            }
            "build" -> {
                args.add("build")
                if (config.target.isNotBlank()) {
                    args.addAll(listOf("--target", config.target))
                }
                if (config.release) {
                    args.add("--release")
                }
            }
            "doctor" -> {
                args.add("doctor")
            }
        }

        return GeneralCommandLine()
            .withExePath(aimerPath)
            .withParameters(args)
            .withWorkDirectory(environment.project.basePath)
            .withParentEnvironmentType(GeneralCommandLine.ParentEnvironmentType.CONSOLE)
    }

    /**
     * Locate the aimer CLI binary.
     * Checks: project target/debug, project target/release, then PATH.
     */
    private fun findAimerCli(): String? {
        val projectDir = environment.project.basePath ?: return null

        val candidates = listOf(
            "$projectDir/target/debug/aimer",
            "$projectDir/target/release/aimer",
            "$projectDir/dev_tools/aimer_cli/target/debug/aimer_cli",
        )

        for (candidate in candidates) {
            if (File(candidate).canExecute()) {
                return candidate
            }
        }

        try {
            val pathResult = ProcessBuilder("which", "aimer")
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
