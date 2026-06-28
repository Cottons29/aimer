package com.cottons.aimer.run

import com.intellij.execution.configurations.ConfigurationFactory
import com.intellij.execution.configurations.ConfigurationType
import com.intellij.execution.configurations.RunConfiguration
import com.intellij.openapi.project.Project

/**
 * Factory that creates [AimerRunConfiguration] instances.
 */
class AimerRunConfigurationFactory(type: ConfigurationType) : ConfigurationFactory(type) {

    override fun getId(): String = AimerRunConfigurationType.ID

    override fun createTemplateConfiguration(project: Project): RunConfiguration {
        return AimerRunConfiguration(project, this, "Aimer")
    }

    override fun getName(): String = "Aimer"
}
