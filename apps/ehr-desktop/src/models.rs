use ehr_core::{Contact, EhrApp, EhrNode, IndexEntry, Role};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

pub(crate) struct Session {
    pub(crate) app: Arc<EhrApp>,
    pub(crate) node: EhrNode,
}

/// State shared by the window callbacks.
#[derive(Default)]
pub(crate) struct View {
    pub(crate) people: Vec<Contact>,
    pub(crate) pending: Vec<Contact>,
    pub(crate) docs: Vec<IndexEntry>,
    pub(crate) selected_person: Option<String>,
    pub(crate) selected_doc: Option<String>,
}

pub(crate) struct State {
    pub(crate) role: Role,
    pub(crate) runtime: tokio::runtime::Runtime,
    pub(crate) data_dir: PathBuf,
    pub(crate) session: Mutex<Option<Arc<Session>>>,
    pub(crate) view: Mutex<View>,
}

impl State {
    pub(crate) fn session(&self) -> Option<Arc<Session>> {
        self.session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) fn view(&self) -> MutexGuard<'_, View> {
        self.view
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn doc_at(&self, index: i32) -> Option<IndexEntry> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.view().docs.get(index).cloned())
    }
}
