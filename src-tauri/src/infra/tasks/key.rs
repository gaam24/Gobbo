//! The single owner of the string ids and the typed identities of long tasks.

use super::model::TaskDedupe;

/// The single owner of the string ids and the typed identities of long tasks. Each named constructor returns the id
/// the workflow already used, together with the [`TaskDedupe`] a command passes to the task registry. No command
/// builds a task id with `format!` any more, and no feature has to re-parse an id to know which rows to merge.
#[derive(Clone)]
pub struct TaskKey {
    id: String,
    dedupe: TaskDedupe,
}

impl TaskKey {
    /// The key of one tool operation: `kind` is [`super::kind::LAUNCH`] or [`super::kind::INSTALL`].
    pub fn tool(kind: &str, game: &str, tool: &str) -> Self {
        Self {
            id: format!("tool:{kind}:{game}:{tool}"),
            dedupe: TaskDedupe::Tool {
                game: game.to_string(),
                tool: tool.to_string(),
            },
        }
    }

    /// The key of the install of one game's loader package.
    pub fn loader(game: &str) -> Self {
        Self {
            id: format!("loader:{game}"),
            dedupe: TaskDedupe::Loader {
                game: game.to_string(),
            },
        }
    }

    /// The key of a deployment of one game's active profile.
    pub fn apply(game: &str) -> Self {
        Self {
            id: format!("apply:{game}"),
            dedupe: TaskDedupe::Apply {
                game: game.to_string(),
            },
        }
    }

    /// The key of one loose archive import, named by game and archive path.
    pub fn import(game: &str, path: &str) -> Self {
        Self {
            id: format!("import:{game}\n{path}"),
            dedupe: TaskDedupe::Import {
                game: game.to_string(),
                path: path.to_string(),
            },
        }
    }

    /// The key of one mod update. The mod id stays in the string for cancellation, but the dedupe group is per game.
    pub fn update_mod(game: &str, id: &str) -> Self {
        Self {
            id: format!("update-mod:{game}\n{id}"),
            dedupe: TaskDedupe::Update {
                game: game.to_string(),
            },
        }
    }

    /// The key of the bulk update of one game.
    pub fn update_all(game: &str) -> Self {
        Self {
            id: format!("update:{game}"),
            dedupe: TaskDedupe::UpdateAll {
                game: game.to_string(),
            },
        }
    }

    /// The key shared by the single and the bulk requirement install of one game. A new install replaces the flag of
    /// the previous one, which is what a retry in the window does.
    pub fn requirement(game: &str) -> Self {
        Self {
            id: format!("requirement:{game}"),
            dedupe: TaskDedupe::Requirement {
                game: game.to_string(),
            },
        }
    }

    /// The key of an automatic collection install, named by game, profile and collection.
    pub fn collection_install(game: &str, profile: &str, collection: &str) -> Self {
        Self {
            id: format!("collection:{game}\n{profile}\n{collection}"),
            dedupe: TaskDedupe::Collection {
                game: game.to_string(),
                profile: profile.to_string(),
                collection: collection.to_string(),
            },
        }
    }

    /// The key of one pack import. A pack download has its own key so the two can be stopped separately.
    pub fn pack_import(game: &str, path: &str) -> Self {
        Self {
            id: format!("pack:{game}\n{path}"),
            dedupe: TaskDedupe::Pack {
                game: game.to_string(),
                path: path.to_string(),
            },
        }
    }

    /// The key of one pack mod download. It shares the dedupe group of the import but has its own cancellation flag.
    pub fn pack_download(game: &str, path: &str) -> Self {
        Self {
            id: format!("pack-download:{game}\n{path}"),
            dedupe: TaskDedupe::Pack {
                game: game.to_string(),
                path: path.to_string(),
            },
        }
    }

    /// The key of one pack link download, named by game and link.
    pub fn pack_link(game: &str, link: &str) -> Self {
        Self {
            id: format!("pack-link:{game}\n{link}"),
            dedupe: TaskDedupe::PackLink {
                game: game.to_string(),
                link: link.to_string(),
            },
        }
    }

    /// The string id that also names the cancellation flag and the journal row.
    pub fn as_str(&self) -> &str {
        &self.id
    }

    /// The typed identity of the operation, used by the window to group lifecycle and progress rows.
    pub const fn dedupe(&self) -> &TaskDedupe {
        &self.dedupe
    }

    /// Builds a key whose id and typed identity deliberately differ. Tests use it to prove the registry trusts the
    /// typed value instead of re-reading the encoding of the id.
    #[cfg(test)]
    pub(crate) const fn with_parts(id: String, dedupe: TaskDedupe) -> Self {
        Self { id, dedupe }
    }

    /// Rebuilds the typed identity from an id. Only kept for the tests and compatibility callers that still pass a
    /// string; the start path hands the registry a [`TaskKey`], so production never reads the encoding back.
    #[cfg(test)]
    fn dedupe_from_id(id: &str) -> TaskDedupe {
        let mut parts = id.split('\n');
        let head = parts.next().unwrap_or("");
        let tail: Vec<&str> = parts.collect();
        let (prefix, game) = head.split_once(':').unwrap_or((head, ""));
        let rest = tail.join("\n");

        match prefix {
            "apply" => TaskDedupe::Apply {
                game: game.to_string(),
            },
            "import" => TaskDedupe::Import {
                game: game.to_string(),
                path: rest,
            },
            "update-mod" => TaskDedupe::Update {
                game: game.to_string(),
            },
            "update" => TaskDedupe::UpdateAll {
                game: game.to_string(),
            },
            "requirement" => TaskDedupe::Requirement {
                game: game.to_string(),
            },
            "loader" => TaskDedupe::Loader {
                game: game.to_string(),
            },
            "collection" => TaskDedupe::Collection {
                game: game.to_string(),
                profile: tail.first().copied().unwrap_or("").to_string(),
                collection: tail.get(1).copied().unwrap_or("").to_string(),
            },
            "pack" | "pack-download" => TaskDedupe::Pack {
                game: game.to_string(),
                path: rest,
            },
            "pack-link" => TaskDedupe::PackLink {
                game: game.to_string(),
                link: rest,
            },
            // `tool:<kind>:<game>:<tool>`: the game and the tool are the last two fields.
            "tool" => game
                .rsplit_once(':')
                .and_then(|(head, tool)| {
                    let (_, game) = head.split_once(':')?;
                    Some(TaskDedupe::Tool {
                        game: game.to_string(),
                        tool: tool.to_string(),
                    })
                })
                .unwrap_or_default(),
            _ => TaskDedupe::Other,
        }
    }
}

#[cfg(test)]
impl From<String> for TaskKey {
    fn from(id: String) -> Self {
        let dedupe = Self::dedupe_from_id(&id);
        Self { id, dedupe }
    }
}

impl From<TaskKey> for String {
    fn from(key: TaskKey) -> Self {
        key.id
    }
}
