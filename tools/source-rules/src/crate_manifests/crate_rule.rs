// SPDX-License-Identifier: GPL-3.0-only
use super::{DependencyTable, InheritedTable};
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum CrateRule {
    InexactWorkspaceDependency {
        name: String,
    },
    UnlistedMember {
        member: String,
    },
    UnlistedWorkspace {
        listing: &'static str,
    },
    ListedWithoutWorkspace {
        directory: String,
    },
    UnknownInheritedTable {
        name: String,
    },
    DifferentWorkspaceTable {
        table: InheritedTable,
    },
    OwnDependency {
        name: String,
    },
    OwnVersion,
    OwnEdition,
    OwnLints,
    VulkanDependency {
        vulkan: String,
        through: String,
        table: DependencyTable,
    },
}

impl fmt::Display for CrateRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InexactWorkspaceDependency { name } => write!(
                formatter,
                "lock the workspace dependency {name} to an exact version `=x.y.z` or give it a path",
            ),
            Self::UnlistedMember { member } => write!(
                formatter,
                "add the member {member} to the workspace dependencies as a path entry",
            ),
            Self::UnlistedWorkspace { listing } => write!(
                formatter,
                "list the directory of this Cargo workspace in the directories of {listing}",
            ),
            Self::ListedWithoutWorkspace { directory } => write!(
                formatter,
                "remove the directory {directory}, which has no Cargo.toml with a [workspace] table",
            ),
            Self::UnknownInheritedTable { name } => write!(
                formatter,
                "replace {name} with a table that a nested workspace copies from the root Cargo.toml: {}",
                InheritedTable::ALL.map(InheritedTable::name).join(", "),
            ),
            Self::DifferentWorkspaceTable { table } => write!(
                formatter,
                "copy workspace.{} of the root Cargo.toml unchanged, which a nested workspace cannot inherit",
                table.name(),
            ),
            Self::OwnDependency { name } => write!(
                formatter,
                "take the dependency {name} from the workspace with `{name}.workspace = true`",
            ),
            Self::OwnVersion => formatter.write_str(
                "take package.version from the workspace with `version.workspace = true`",
            ),
            Self::OwnEdition => formatter.write_str(
                "take package.edition from the workspace with `edition.workspace = true`",
            ),
            Self::OwnLints => formatter.write_str(
                "take the lints from the workspace with `[lints]` and `workspace = true`",
            ),
            Self::VulkanDependency {
                vulkan,
                through,
                table: DependencyTable::Dependencies,
            } => write!(
                formatter,
                "remove the dependency {through}, which adds {vulkan} to a crate that builds without Vulkan",
            ),
            Self::VulkanDependency {
                vulkan,
                through,
                table: DependencyTable::BuildDependencies,
            } => write!(
                formatter,
                "remove the build-dependency {through}, which adds {vulkan} to the build of a crate that builds without Vulkan",
            ),
            Self::VulkanDependency {
                vulkan,
                through,
                table: DependencyTable::DevDependencies,
            } => write!(
                formatter,
                "remove the dev-dependency {through}, which adds {vulkan} to the tests of a crate whose tests build without Vulkan",
            ),
        }
    }
}
