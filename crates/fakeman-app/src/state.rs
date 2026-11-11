//! The app's state: the saved endpoint list, the open tabs and the selected
//! endpoint's draft, with autosave to the store.

use std::time::Duration;

use anyhow::{Context as _, Result};
use fakeman_core::Endpoint;
use fakeman_store::{EndpointId, EndpointSummary, Store};
use gpui_kit::{Context, EventEmitter, Task};

use crate::mapping::{endpoint_from_record, record_from_endpoint};

/// How long the draft has to stay unchanged before it is written.
pub const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

pub const NEW_ENDPOINT_NAME: &str = "New endpoint";

/// A failure the window shows as a notification.
pub struct StateError(pub String);

pub struct AppState {
    store: Store,
    endpoints: Vec<EndpointSummary>,
    open_tabs: Vec<EndpointId>,
    draft: Option<Endpoint>,
    dirty: bool,
    pending_save: Option<Task<()>>,
}

impl EventEmitter<StateError> for AppState {}

impl AppState {
    /// Loads the endpoint list and opens the first endpoint, if any.
    pub fn load(store: Store) -> Result<Self> {
        let mut state = Self {
            endpoints: store.list_endpoints().context("loading endpoints")?,
            store,
            open_tabs: Vec::new(),
            draft: None,
            dirty: false,
            pending_save: None,
        };
        if let Some(first) = state.endpoints.first() {
            state.select(first.id)?;
        }
        Ok(state)
    }

    pub fn endpoints(&self) -> &[EndpointSummary] {
        &self.endpoints
    }

    pub fn endpoint(&self, id: EndpointId) -> Option<&EndpointSummary> {
        self.endpoints.iter().find(|e| e.id == id)
    }

    /// Endpoints whose name or URL contains `query`, ignoring case.
    pub fn filtered(&self, query: &str) -> Vec<&EndpointSummary> {
        let query = query.trim().to_lowercase();
        self.endpoints
            .iter()
            .filter(|e| {
                query.is_empty()
                    || e.name.to_lowercase().contains(&query)
                    || e.url.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub fn open_tabs(&self) -> &[EndpointId] {
        &self.open_tabs
    }

    pub fn draft(&self) -> Option<&Endpoint> {
        self.draft.as_ref()
    }

    pub fn selected(&self) -> Option<EndpointId> {
        self.draft.as_ref().map(|d| d.id.0)
    }

    /// Saves the current draft first, then opens `id` in a tab.
    pub fn select(&mut self, id: EndpointId) -> Result<()> {
        if self.selected() == Some(id) {
            return Ok(());
        }
        self.flush()?;
        let record = self
            .store
            .get_endpoint(id)
            .with_context(|| format!("opening endpoint {id}"))?;
        self.draft = Some(endpoint_from_record(record));
        if !self.open_tabs.contains(&id) {
            self.open_tabs.push(id);
        }
        Ok(())
    }

    pub fn create(&mut self) -> Result<EndpointId> {
        self.flush()?;
        let record = self
            .store
            .create(NEW_ENDPOINT_NAME)
            .context("creating an endpoint")?;
        self.reload_list()?;
        self.select(record.id)?;
        Ok(record.id)
    }

    /// Renames an endpoint. A blank name keeps the old one.
    pub fn rename(&mut self, id: EndpointId, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || self.endpoint(id).is_some_and(|e| e.name == name) {
            return Ok(());
        }
        self.store
            .rename(id, name)
            .with_context(|| format!("renaming endpoint {id}"))?;
        if let Some(summary) = self.endpoints.iter_mut().find(|e| e.id == id) {
            summary.name = name.to_owned();
        }
        if let Some(draft) = self.draft.as_mut().filter(|d| d.id.0 == id) {
            draft.name = name.to_owned();
        }
        Ok(())
    }

    /// Copies an endpoint, including unsaved edits, and opens the copy.
    pub fn duplicate(&mut self, id: EndpointId) -> Result<EndpointId> {
        self.flush()?;
        let copy = self
            .store
            .duplicate(id)
            .with_context(|| format!("duplicating endpoint {id}"))?;
        self.reload_list()?;
        self.select(copy.id)?;
        Ok(copy.id)
    }

    /// Deletes an endpoint. If it was open, the neighbouring tab (or the
    /// neighbouring list entry) is opened instead.
    pub fn delete(&mut self, id: EndpointId) -> Result<()> {
        let was_selected = self.selected() == Some(id);
        if was_selected {
            self.draft = None;
            self.dirty = false;
            self.pending_save = None;
        }
        let list_ix = self.endpoints.iter().position(|e| e.id == id);
        self.store
            .delete(id)
            .with_context(|| format!("deleting endpoint {id}"))?;
        self.reload_list()?;
        let tab_ix = self.open_tabs.iter().position(|t| *t == id);
        self.open_tabs.retain(|t| *t != id);
        if !was_selected {
            return Ok(());
        }
        let next = tab_ix
            .and_then(|ix| neighbour(&self.open_tabs, ix))
            .or_else(|| list_ix.and_then(|ix| neighbour(&self.endpoints, ix).map(|e| e.id)));
        match next {
            Some(next) => self.select(next),
            None => Ok(()),
        }
    }

    /// Closes a tab, saving first. Closing the active tab opens its neighbour.
    pub fn close_tab(&mut self, id: EndpointId) -> Result<()> {
        let Some(ix) = self.open_tabs.iter().position(|t| *t == id) else {
            return Ok(());
        };
        if self.selected() == Some(id) {
            self.flush()?;
            self.draft = None;
        }
        self.open_tabs.remove(ix);
        match neighbour(&self.open_tabs, ix) {
            Some(next) if self.draft.is_none() => self.select(next),
            _ => Ok(()),
        }
    }

    /// Changes the draft's URL; the caller schedules the save.
    pub fn set_url(&mut self, url: &str) {
        let Some(draft) = self.draft.as_mut().filter(|d| d.url != url) else {
            return;
        };
        draft.url = url.to_owned();
        let id = draft.id.0;
        if let Some(summary) = self.endpoints.iter_mut().find(|e| e.id == id) {
            summary.url = url.to_owned();
        }
        self.dirty = true;
    }

    /// Writes the draft if it has unsaved changes.
    pub fn flush(&mut self) -> Result<()> {
        self.pending_save = None;
        let Some(draft) = self.draft.as_ref().filter(|_| self.dirty) else {
            return Ok(());
        };
        self.store
            .save(&record_from_endpoint(draft))
            .with_context(|| format!("saving \"{}\"", draft.name))?;
        self.dirty = false;
        Ok(())
    }

    /// Saves the draft once it has been unchanged for [`SAVE_DEBOUNCE`].
    /// Each call restarts the countdown.
    pub fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.pending_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            this.update(cx, |state, cx| state.flush_or_report(cx)).ok();
        }));
    }

    /// Like [`Self::flush`], but reports a failure to the window.
    pub fn flush_or_report(&mut self, cx: &mut Context<Self>) {
        if let Err(err) = self.flush() {
            cx.emit(StateError(format!("{err:#}")));
        }
    }

    fn reload_list(&mut self) -> Result<()> {
        let mut endpoints = self.store.list_endpoints().context("loading endpoints")?;
        if let Some(draft) = self.draft.as_ref()
            && let Some(summary) = endpoints.iter_mut().find(|e| e.id == draft.id.0)
        {
            summary.url = draft.url.clone();
        }
        self.endpoints = endpoints;
        Ok(())
    }
}

/// The item that takes the place of the one removed at `ix`: the next one,
/// or the previous one at the end of the list.
fn neighbour<T: Clone>(items: &[T], ix: usize) -> Option<T> {
    items
        .get(ix)
        .or_else(|| items.get(ix.checked_sub(1)?))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(state: &AppState) -> Vec<&str> {
        state.endpoints().iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn opens_the_first_endpoint_of_a_fresh_install() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load(Store::open(dir.path().join("db")).unwrap()).unwrap();
        assert_eq!(names(&state), ["Create user"]);
        let draft = state.draft().unwrap();
        assert_eq!(draft.url, "https://httpbin.org/post");
        assert_eq!(draft.fields.len(), 3);
        assert_eq!(state.open_tabs(), [draft.id.0]);
    }

    #[test]
    fn empty_database_has_no_selection() {
        let state = AppState::load(Store::open_in_memory().unwrap()).unwrap();
        assert!(state.endpoints().is_empty());
        assert!(state.draft().is_none());
    }

    #[test]
    fn sidebar_and_url_edits_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fakeman.db");
        let mut state = AppState::load(Store::open(&path).unwrap()).unwrap();
        let seed = state.selected().unwrap();

        let created = state.create().unwrap();
        assert_eq!(state.selected(), Some(created));
        state.set_url("https://example.com/new");
        state.rename(created, "  Signup  ").unwrap();
        let copy = state.duplicate(created).unwrap();
        assert_eq!(state.draft().unwrap().url, "https://example.com/new");
        state.set_url("https://example.com/copy");
        state.select(seed).unwrap();
        state.delete(seed).unwrap();
        state.select(copy).unwrap();
        state.set_url("https://example.com/copy2");
        state.flush().unwrap();
        drop(state);

        let state = AppState::load(Store::open(&path).unwrap()).unwrap();
        assert_eq!(names(&state), ["Signup", "Signup copy"]);
        let urls: Vec<_> = state.endpoints().iter().map(|e| e.url.as_str()).collect();
        assert_eq!(
            urls,
            ["https://example.com/new", "https://example.com/copy2"]
        );
    }

    #[test]
    fn switching_endpoints_saves_the_draft() {
        let mut state = AppState::load(Store::open_in_memory().unwrap()).unwrap();
        let a = state.create().unwrap();
        state.set_url("https://a.test");
        let b = state.create().unwrap();
        assert_eq!(state.draft().unwrap().url, "");
        state.set_url("https://b.test");
        state.select(a).unwrap();
        assert_eq!(state.draft().unwrap().url, "https://a.test");
        state.select(b).unwrap();
        assert_eq!(state.draft().unwrap().url, "https://b.test");
    }

    #[test]
    fn blank_rename_keeps_the_name() {
        let mut state = AppState::load(Store::open_in_memory().unwrap()).unwrap();
        let id = state.create().unwrap();
        state.rename(id, "   ").unwrap();
        assert_eq!(names(&state), [NEW_ENDPOINT_NAME]);
        assert_eq!(state.draft().unwrap().name, NEW_ENDPOINT_NAME);
    }

    #[test]
    fn deleting_the_open_endpoint_opens_a_neighbour() {
        let mut state = AppState::load(Store::open_in_memory().unwrap()).unwrap();
        let a = state.create().unwrap();
        let b = state.create().unwrap();
        let c = state.create().unwrap();
        state.select(b).unwrap();
        state.delete(b).unwrap();
        assert_eq!(state.selected(), Some(c));
        assert_eq!(state.open_tabs(), [a, c]);
        state.delete(c).unwrap();
        assert_eq!(state.selected(), Some(a));
        state.delete(a).unwrap();
        assert_eq!(state.selected(), None);
        assert!(state.open_tabs().is_empty());
    }

    #[test]
    fn deleting_another_endpoint_keeps_the_selection() {
        let mut state = AppState::load(Store::open_in_memory().unwrap()).unwrap();
        let a = state.create().unwrap();
        let b = state.create().unwrap();
        state.set_url("https://b.test");
        state.delete(a).unwrap();
        assert_eq!(state.selected(), Some(b));
        assert_eq!(state.draft().unwrap().url, "https://b.test");
    }

    #[test]
    fn closing_the_active_tab_opens_its_neighbour() {
        let mut state = AppState::load(Store::open_in_memory().unwrap()).unwrap();
        let a = state.create().unwrap();
        let b = state.create().unwrap();
        state.set_url("https://b.test");
        state.close_tab(b).unwrap();
        assert_eq!(state.selected(), Some(a));
        assert_eq!(state.open_tabs(), [a]);
        state.select(b).unwrap();
        assert_eq!(state.draft().unwrap().url, "https://b.test");
    }

    #[test]
    fn filter_matches_name_or_url_ignoring_case() {
        let mut state = AppState::load(Store::open_in_memory().unwrap()).unwrap();
        let a = state.create().unwrap();
        state.rename(a, "Create user").unwrap();
        state.create().unwrap();
        state.set_url("https://shop.test/orders");
        let found =
            |q: &str| -> Vec<String> { state.filtered(q).iter().map(|e| e.name.clone()).collect() };
        assert_eq!(found("USER"), ["Create user"]);
        assert_eq!(found("orders"), [NEW_ENDPOINT_NAME]);
        assert_eq!(found("  ").len(), 2);
        assert!(found("nothing").is_empty());
    }
}
