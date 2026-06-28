package com.cottons.aimer.run

import com.intellij.execution.configurations.ConfigurationTypeBase
import com.intellij.icons.AllIcons

/**
 * Registers the "Aimer" run configuration type in the IDE.
 */
class AimerRunConfigurationType : ConfigurationTypeBase(
    ID,
    "Aimer",
    "Run Aimer applications and assemble bundles",
    AllIcons.Actions.Execute
) {
    companion object {
        const val ID = "AimerRunConfiguration"
    }

    init {
        addFactory(AimerRunConfigurationFactory(this))
    }
}
