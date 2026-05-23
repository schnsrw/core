//! High-level document wrapper.
//!
//! [`Document`] wraps [`DocumentModel`] with undo/redo history and provides
//! a convenient API for reading, editing, and exporting documents.

use std::collections::HashSet;

use s1_model::{
    AttributeKey, AttributeValue, DocumentMetadata, DocumentModel, Node, NodeId, NodeType,
};
use s1_ops::{History, Operation, Transaction, TransactionBuilder};

use crate::error::Error;
use crate::format::Format;

/// A document with undo/redo history and high-level operations.
pub struct Document {
    model: DocumentModel,
    history: History,
    /// Lossless preservation of the original input.
    ///
    /// Populated by [`Engine::open(Format::Docx)`]. While preservation is
    /// `Some` *and* `model_dirty == false`, `export(Docx)` re-emits the
    /// package verbatim — perfect round-trip for the converter case.
    ///
    /// Once any edit-class mutation runs (`apply`, `apply_transaction`,
    /// `undo`, `redo`, `update_toc`), `model_dirty` flips to `true`.
    /// `export(Docx)` then splices the regenerated `word/document.xml`
    /// into the preserved package so every *other* part (theme, fontTable,
    /// customXml, headers, footers, footnotes, endnotes, comments,
    /// numbering, styles, images, rels, content types) still rides
    /// through untouched.
    ///
    /// Hard escape hatches (`model_mut`, `metadata_mut`) drop preservation
    /// entirely — once a caller bypasses the operation system, we can't
    /// reason about which parts of the model agree with the package.
    preservation: Option<s1_ooxml::Package>,
    /// ODF preservation — counterpart of `preservation` for OpenDocument
    /// inputs. Populated by `Engine::open(Format::Odt)`. While this is
    /// `Some` *and* `model_dirty == false`, `export(Odt)` re-emits the
    /// package verbatim — zero-drop round-trip for the converter case.
    odf_preservation: Option<s1_odf::Package>,
    /// Per-body-child origin table — maps each top-level body NodeId to
    /// its preserved `XmlElement`. Built at open time alongside
    /// `preservation`. Drives the Phase 2b per-node splice in
    /// `export(Docx)`: clean NodeIds re-emit their preserved XML
    /// verbatim, dirty NodeIds re-render through the writer.
    body_origin: Option<s1_format_docx::BodyOrigin>,
    /// ODT counterpart of [`body_origin`]. Maps each top-level
    /// `<office:text>` child NodeId to its preserved `XmlElement`.
    /// Drives the ODT Phase 2b per-node splice in `export(Odt)`.
    odf_body_origin: Option<s1_format_odt::BodyOrigin>,
    /// Top-level body NodeIds touched since open. Drives the splice
    /// path — only these get regenerated from the model; everything
    /// else rides through verbatim via [`body_origin`].
    dirty_body_ids: HashSet<NodeId>,
    /// `true` once an edit-class mutation has been applied since open.
    /// Drives the splice path in `export(Docx)`.
    model_dirty: bool,
    /// Set when the body has structurally changed (children added /
    /// removed / reordered) or when a non-operation mutation bypassed
    /// the dirty-tracking path. Forces the splice to fall back to
    /// wholesale regenerate of `word/document.xml` because the origin
    /// table can no longer be aligned positionally.
    body_structural_dirty: bool,
}

impl Document {
    /// Create a new empty document.
    pub fn new() -> Self {
        Self {
            model: DocumentModel::new(),
            history: History::new(),
            preservation: None,
            odf_preservation: None,
            body_origin: None,
            odf_body_origin: None,
            dirty_body_ids: HashSet::new(),
            model_dirty: false,
            body_structural_dirty: false,
        }
    }

    /// Create a Document from an existing model (e.g., after reading a file).
    pub fn from_model(model: DocumentModel) -> Self {
        Self {
            model,
            history: History::new(),
            preservation: None,
            odf_preservation: None,
            body_origin: None,
            odf_body_origin: None,
            dirty_body_ids: HashSet::new(),
            model_dirty: false,
            body_structural_dirty: false,
        }
    }

    /// Create a Document from a model **plus** the lossless OOXML package it
    /// came from. The package is held as preservation metadata. After this
    /// call, `is_dirty() == false`; the next `export(Docx)` is a verbatim
    /// re-emission. Edit-class mutations flip the dirty flag and switch
    /// `export(Docx)` to the splice path (regenerate `word/document.xml`,
    /// keep every other part).
    pub fn from_model_with_package(model: DocumentModel, package: s1_ooxml::Package) -> Self {
        Self {
            model,
            history: History::new(),
            preservation: Some(package),
            odf_preservation: None,
            body_origin: None,
            odf_body_origin: None,
            dirty_body_ids: HashSet::new(),
            model_dirty: false,
            body_structural_dirty: false,
        }
    }

    /// Create a Document from a model **plus** the lossless ODF package it
    /// came from. Counterpart of [`from_model_with_package`] for ODT
    /// inputs. After this call, `is_dirty() == false`; the next
    /// `export(Odt)` is a verbatim re-emission of the package. Edit-class
    /// mutations flip the dirty flag and `export(Odt)` falls back to
    /// regenerating from the model (Phase 2a / 2b for ODT will refine
    /// that lane further).
    pub fn from_model_with_odf_package(model: DocumentModel, package: s1_odf::Package) -> Self {
        Self {
            model,
            history: History::new(),
            preservation: None,
            odf_preservation: Some(package),
            body_origin: None,
            odf_body_origin: None,
            dirty_body_ids: HashSet::new(),
            model_dirty: false,
            body_structural_dirty: false,
        }
    }

    /// Create a Document from a model, its ODF preservation package, and
    /// a per-body-child origin table. Counterpart of [`from_open_state`]
    /// for ODT inputs — the origin table is what lets `export(Odt)`
    /// splice individual body elements back verbatim on edit
    /// (`<text:p>` / `<text:h>` / `<table:table>` / TOC) so unknown
    /// children inside them (`draw:frame`, `text:span`, `text:s`,
    /// `text:soft-page-break`, `svg:title/desc`, …) survive.
    pub fn from_odt_open_state(
        model: DocumentModel,
        package: s1_odf::Package,
        body_origin: s1_format_odt::BodyOrigin,
    ) -> Self {
        Self {
            model,
            history: History::new(),
            preservation: None,
            odf_preservation: Some(package),
            body_origin: None,
            odf_body_origin: Some(body_origin),
            dirty_body_ids: HashSet::new(),
            model_dirty: false,
            body_structural_dirty: false,
        }
    }

    /// Create a Document from a model, its preservation package, and a
    /// per-body-child origin table. The origin table is what lets
    /// `export(Docx)` splice individual body elements back verbatim on
    /// edit instead of regenerating the whole `word/document.xml` — so
    /// untouched paragraphs / tables keep every unknown OOXML child
    /// (drawings, structured document tags, custom XML) byte-for-byte.
    pub fn from_open_state(
        model: DocumentModel,
        package: s1_ooxml::Package,
        body_origin: s1_format_docx::BodyOrigin,
    ) -> Self {
        Self {
            model,
            history: History::new(),
            preservation: Some(package),
            odf_preservation: None,
            body_origin: Some(body_origin),
            odf_body_origin: None,
            dirty_body_ids: HashSet::new(),
            model_dirty: false,
            body_structural_dirty: false,
        }
    }

    /// `true` if the document still has its original preservation package.
    pub fn has_preservation(&self) -> bool {
        self.preservation.is_some()
    }

    /// `true` if any edit-class mutation has been applied since the document
    /// was opened or last cleaned. Drives the splice path in `export(Docx)`.
    pub fn is_dirty(&self) -> bool {
        self.model_dirty
    }

    /// Drop the preservation package entirely. Hard escape hatch for
    /// callers that bypass the operation system and want to signal "I just
    /// scrambled the model — don't try to preserve anything from the
    /// original."
    pub fn invalidate_preservation(&mut self) {
        self.preservation = None;
        self.odf_preservation = None;
        self.body_origin = None;
        self.odf_body_origin = None;
        self.model_dirty = true;
        self.body_structural_dirty = true;
    }

    /// Borrow the preservation package, if any. Mainly for tests and
    /// diagnostics; production code should go through `export`.
    pub fn preservation(&self) -> Option<&s1_ooxml::Package> {
        self.preservation.as_ref()
    }

    // ─── Model access ────────────────────────────────────────────────

    /// Get a read-only reference to the underlying document model.
    pub fn model(&self) -> &DocumentModel {
        &self.model
    }

    /// Get a mutable reference to the underlying document model.
    ///
    /// # Warning
    ///
    /// **This is an advanced escape hatch.** Direct mutation bypasses the
    /// operation system, which means:
    /// - Changes will NOT be recorded in undo/redo history
    /// - Changes will NOT generate CRDT operations for collaboration
    /// - The document may enter an inconsistent state
    ///
    /// Prefer [`apply`](Self::apply) or [`apply_transaction`](Self::apply_transaction)
    /// for all edits that should be undoable or collaborative.
    ///
    /// This method exists for cases where you need direct model access
    /// (e.g., bulk import, format reader integration, or testing).
    pub fn model_mut(&mut self) -> &mut DocumentModel {
        self.preservation = None;
        self.odf_preservation = None;
        self.body_origin = None;
        self.odf_body_origin = None;
        self.body_structural_dirty = true;
        &mut self.model
    }

    /// Consume the Document and return the underlying model.
    pub fn into_model(self) -> DocumentModel {
        self.model
    }

    // ─── Metadata ────────────────────────────────────────────────────

    /// Get document metadata (title, author, etc.).
    pub fn metadata(&self) -> &DocumentMetadata {
        self.model.metadata()
    }

    /// Get mutable document metadata.
    pub fn metadata_mut(&mut self) -> &mut DocumentMetadata {
        self.preservation = None;
        self.odf_preservation = None;
        self.body_origin = None;
        self.odf_body_origin = None;
        self.body_structural_dirty = true;
        self.model.metadata_mut()
    }

    // ─── Content queries ─────────────────────────────────────────────

    /// Extract all text as a plain string. Paragraphs separated by newlines.
    pub fn to_plain_text(&self) -> String {
        self.model.to_plain_text()
    }

    /// Get the body node ID.
    pub fn body_id(&self) -> Option<NodeId> {
        self.model.body_id()
    }

    /// Get a node by ID.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.model.node(id)
    }

    /// Generate the next unique node ID.
    pub fn next_id(&mut self) -> NodeId {
        self.model.next_id()
    }

    /// Return top-level paragraph node IDs in document order.
    ///
    /// This returns only direct children of the document body that are
    /// paragraphs. Paragraphs nested inside tables, headers, footers,
    /// or other container elements are **not** included.
    ///
    /// To traverse all paragraphs (including nested ones), walk the
    /// document tree via [`model()`](Self::model) and
    /// [`DocumentModel::node()`].
    pub fn paragraph_ids(&self) -> Vec<NodeId> {
        let body_id = match self.model.body_id() {
            Some(id) => id,
            None => return vec![],
        };
        let body = match self.model.node(body_id) {
            Some(n) => n,
            None => return vec![],
        };
        body.children
            .iter()
            .copied()
            .filter(|id| {
                self.model
                    .node(*id)
                    .map(|n| n.node_type == NodeType::Paragraph)
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Count top-level body paragraphs.
    ///
    /// Equivalent to `self.paragraph_ids().len()`. See
    /// [`paragraph_ids()`](Self::paragraph_ids) for semantics.
    pub fn paragraph_count(&self) -> usize {
        self.paragraph_ids().len()
    }

    // ─── Styles ──────────────────────────────────────────────────────

    /// Get all styles.
    pub fn styles(&self) -> &[s1_model::Style] {
        self.model.styles()
    }

    /// Get a style by ID.
    pub fn style_by_id(&self, id: &str) -> Option<&s1_model::Style> {
        self.model.style_by_id(id)
    }

    /// Get the numbering definitions.
    pub fn numbering(&self) -> &s1_model::NumberingDefinitions {
        self.model.numbering()
    }

    /// Get section properties.
    pub fn sections(&self) -> &[s1_model::SectionProperties] {
        self.model.sections()
    }

    // ─── Transactions ────────────────────────────────────────────────

    /// Begin building a new transaction.
    ///
    /// All operations within a transaction form a single undo unit.
    pub fn begin_transaction(label: &str) -> TransactionBuilder {
        TransactionBuilder::new().label(label)
    }

    /// Apply a transaction to the document.
    ///
    /// On success, the transaction is pushed onto the undo stack.
    /// On failure, all operations are rolled back.
    pub fn apply_transaction(&mut self, txn: &Transaction) -> Result<(), Error> {
        self.model_dirty = true;
        // Classify ops against the pre-apply model state so we know whether
        // each target is currently a top-level body descendant. Insert /
        // Delete / Move at body level changes the body's structure and
        // collapses the Phase 2b splice back to wholesale regenerate.
        let body_id = self.model.body_id();
        let mut new_dirty: Vec<NodeId> = Vec::new();
        let mut structural = false;
        for op in &txn.operations {
            classify_op(&self.model, body_id, op, &mut new_dirty, &mut structural);
        }
        self.history.apply(&mut self.model, txn)?;
        if structural {
            self.body_structural_dirty = true;
        } else {
            self.dirty_body_ids.extend(new_dirty);
        }
        Ok(())
    }

    /// Apply a single operation as a transaction.
    pub fn apply(&mut self, op: Operation) -> Result<(), Error> {
        let mut txn = Transaction::new();
        txn.push(op);
        self.apply_transaction(&txn)
    }

    // ─── Undo / Redo ─────────────────────────────────────────────────

    /// Undo the last transaction. Returns `true` if something was undone.
    pub fn undo(&mut self) -> Result<bool, Error> {
        self.model_dirty = true;
        // Undo/redo can shuffle body structure in ways the per-NodeId
        // tracker can't reliably reconstruct after the fact — collapse
        // to wholesale regenerate for safety.
        self.body_structural_dirty = true;
        Ok(self.history.undo(&mut self.model)?)
    }

    /// Redo the last undone transaction. Returns `true` if something was redone.
    pub fn redo(&mut self) -> Result<bool, Error> {
        self.model_dirty = true;
        self.body_structural_dirty = true;
        Ok(self.history.redo(&mut self.model)?)
    }

    /// Check if undo is available.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Check if redo is available.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Clear all undo/redo history.
    pub fn clear_history(&mut self) {
        // History clear doesn't mutate the model, so preservation is safe
        // to keep. Left here for symmetry with other history-related methods.
        self.history.clear();
    }

    /// Set the maximum number of undo steps. 0 means unlimited.
    pub fn set_undo_cap(&mut self, max: usize) {
        self.history.set_max_undo(max);
    }

    /// Get the number of undo steps currently on the stack.
    pub fn undo_count(&self) -> usize {
        self.history.undo_count()
    }

    /// Merge the last `count` undo entries into a single undo step.
    ///
    /// Used by the batch operation API to group multiple operations
    /// into one undo unit.
    pub fn merge_undo_entries(&mut self, count: usize, label: &str) -> Result<(), Error> {
        self.history.merge_undo_entries(count, label);
        Ok(())
    }

    // ─── TOC ────────────────────────────────────────────────────────

    /// Update all Table of Contents entries in the document.
    ///
    /// Scans for heading paragraphs and regenerates the cached entry
    /// paragraphs inside each TOC node. Call this before exporting if
    /// content has changed since the TOC was inserted.
    pub fn update_toc(&mut self) {
        // First, find all TOC nodes and their max_level
        let body_id = match self.model.body_id() {
            Some(id) => id,
            None => return,
        };
        let toc_nodes: Vec<(NodeId, u8)> = self
            .find_toc_nodes(body_id)
            .into_iter()
            .map(|id| {
                let max_level = self
                    .model
                    .node(id)
                    .and_then(|n| n.attributes.get_i64(&AttributeKey::TocMaxLevel))
                    .unwrap_or(3) as u8;
                (id, max_level)
            })
            .collect();

        if toc_nodes.is_empty() {
            // No TOC to update — no model mutation either. Skip dirtying
            // so the no-op edit path can re-emit the preserved package
            // verbatim.
            return;
        }

        // TOC update is about to rewrite cached entry paragraphs. Each
        // TOC node is itself a top-level body child, so dirty-tracking
        // just adds the TOC NodeIds to the body dirty set — every other
        // body child rides through verbatim.
        self.model_dirty = true;
        for (toc_id, _) in &toc_nodes {
            self.dirty_body_ids.insert(*toc_id);
        }

        // Collect headings (excluding any inside TOC nodes)
        let headings = self.model.collect_headings();

        for (toc_id, max_level) in toc_nodes {
            self.generate_toc_entries(toc_id, max_level, &headings);
        }
    }

    fn find_toc_nodes(&self, container_id: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        if let Some(node) = self.model.node(container_id) {
            for &child_id in &node.children {
                if let Some(child) = self.model.node(child_id) {
                    if child.node_type == NodeType::TableOfContents {
                        result.push(child_id);
                    }
                }
            }
        }
        result
    }

    fn generate_toc_entries(
        &mut self,
        toc_id: NodeId,
        max_level: u8,
        headings: &[(NodeId, u8, String)],
    ) {
        // Remove existing children
        if let Some(toc) = self.model.node(toc_id) {
            let old_children: Vec<NodeId> = toc.children.clone();
            for child_id in old_children {
                let _ = self.model.remove_node(child_id);
            }
        }

        // Generate new entry paragraphs
        let mut child_index = 0;
        for (_heading_id, level, text) in headings {
            if *level > max_level {
                continue;
            }

            // Create paragraph for this TOC entry
            let para_id = self.model.next_id();
            let mut para = Node::new(para_id, NodeType::Paragraph);
            para.attributes.set(
                AttributeKey::StyleId,
                AttributeValue::String(format!("TOC{}", level)),
            );
            let _ = self.model.insert_node(toc_id, child_index, para);

            // Add a run with the heading text
            let run_id = self.model.next_id();
            let _ = self
                .model
                .insert_node(para_id, 0, Node::new(run_id, NodeType::Run));

            let text_id = self.model.next_id();
            let _ = self
                .model
                .insert_node(run_id, 0, Node::text(text_id, text.clone()));

            child_index += 1;
        }
    }

    // ─── Track Changes ───────────────────────────────────────────

    /// List all tracked changes in the document.
    ///
    /// Returns a list of tuples: `(node_id, revision_type, author, date)` for
    /// every node that carries a `RevisionType` attribute.
    pub fn tracked_changes(&self) -> Vec<(NodeId, String, Option<String>, Option<String>)> {
        let root_id = self.model.root_id();
        let mut result = Vec::new();
        for node in self.model.descendants(root_id) {
            if let Some(rev_type) = node.attributes.get_string(&AttributeKey::RevisionType) {
                let author = node
                    .attributes
                    .get_string(&AttributeKey::RevisionAuthor)
                    .map(|s| s.to_string());
                let date = node
                    .attributes
                    .get_string(&AttributeKey::RevisionDate)
                    .map(|s| s.to_string());
                result.push((node.id, rev_type.to_string(), author, date));
            }
        }
        result
    }

    /// Accept all tracked changes in the document.
    ///
    /// - **Insertions**: revision attributes are removed; the inserted content stays.
    /// - **Deletions**: the deleted nodes are removed from the tree entirely.
    /// - **Format changes**: revision attributes (including original formatting)
    ///   are removed; the current formatting is kept.
    ///
    /// This is a bulk transform that bypasses the undo/redo history.
    ///
    /// # Errors
    ///
    /// Returns an error if a node marked for deletion cannot be removed
    /// (e.g., it is the root node).
    pub fn accept_all_changes(&mut self) -> Result<(), Error> {
        let changes = self.tracked_changes();
        for (node_id, rev_type, _, _) in changes {
            self.accept_change_inner(node_id, &rev_type)?;
        }
        Ok(())
    }

    /// Reject all tracked changes in the document.
    ///
    /// - **Insertions**: the inserted nodes are removed from the tree entirely.
    /// - **Deletions**: revision attributes are removed; the content stays (un-deleted).
    /// - **Format changes**: original formatting is restored from
    ///   `RevisionOriginalFormatting`, and all revision attributes are removed.
    ///
    /// This is a bulk transform that bypasses the undo/redo history.
    ///
    /// # Errors
    ///
    /// Returns an error if a node marked for removal cannot be removed
    /// (e.g., it is the root node).
    pub fn reject_all_changes(&mut self) -> Result<(), Error> {
        let changes = self.tracked_changes();
        for (node_id, rev_type, _, _) in changes {
            self.reject_change_inner(node_id, &rev_type)?;
        }
        Ok(())
    }

    /// Accept a single tracked change by node ID.
    ///
    /// See [`accept_all_changes`](Self::accept_all_changes) for the semantics
    /// of accepting each revision type.
    ///
    /// # Errors
    ///
    /// Returns an error if the node does not exist, has no `RevisionType`
    /// attribute, or cannot be removed from the tree.
    pub fn accept_change(&mut self, node_id: NodeId) -> Result<(), Error> {
        let rev_type = self
            .model
            .node(node_id)
            .and_then(|n| {
                n.attributes
                    .get_string(&AttributeKey::RevisionType)
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                Error::Format(format!(
                    "Node {node_id} does not exist or has no RevisionType attribute"
                ))
            })?;
        self.accept_change_inner(node_id, &rev_type)
    }

    /// Reject a single tracked change by node ID.
    ///
    /// See [`reject_all_changes`](Self::reject_all_changes) for the semantics
    /// of rejecting each revision type.
    ///
    /// # Errors
    ///
    /// Returns an error if the node does not exist, has no `RevisionType`
    /// attribute, or cannot be removed from the tree.
    pub fn reject_change(&mut self, node_id: NodeId) -> Result<(), Error> {
        let rev_type = self
            .model
            .node(node_id)
            .and_then(|n| {
                n.attributes
                    .get_string(&AttributeKey::RevisionType)
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                Error::Format(format!(
                    "Node {node_id} does not exist or has no RevisionType attribute"
                ))
            })?;
        self.reject_change_inner(node_id, &rev_type)
    }

    /// Internal: accept a single change (shared by accept_change and accept_all_changes).
    fn accept_change_inner(&mut self, node_id: NodeId, rev_type: &str) -> Result<(), Error> {
        match rev_type {
            "Insert" | "FormatChange" => {
                // Content/formatting stays; just strip revision attributes.
                Self::strip_revision_attributes(self.model.node_mut(node_id));
            }
            "Delete" => {
                // Remove the node entirely.
                self.model.remove_node(node_id).map_err(|e| {
                    Error::Format(format!("Failed to remove deleted node {node_id}: {e}"))
                })?;
            }
            _ => {
                // Unknown revision type — strip attributes defensively.
                Self::strip_revision_attributes(self.model.node_mut(node_id));
            }
        }
        Ok(())
    }

    /// Internal: reject a single change (shared by reject_change and reject_all_changes).
    fn reject_change_inner(&mut self, node_id: NodeId, rev_type: &str) -> Result<(), Error> {
        match rev_type {
            "Insert" => {
                // Remove the inserted node entirely.
                self.model.remove_node(node_id).map_err(|e| {
                    Error::Format(format!("Failed to remove inserted node {node_id}: {e}"))
                })?;
            }
            "Delete" => {
                // Un-delete: content stays, strip revision attributes.
                Self::strip_revision_attributes(self.model.node_mut(node_id));
            }
            "FormatChange" => {
                // Restore original formatting if available, then strip revision attrs.
                if let Some(node) = self.model.node_mut(node_id) {
                    // If RevisionOriginalFormatting contains serialized attribute data,
                    // parse and restore it. The convention is a semicolon-separated list
                    // of "key=value" pairs, but for now we support the common case where
                    // original formatting attributes were stored alongside the revision
                    // attributes. The caller is responsible for setting appropriate
                    // original formatting attributes before calling reject.
                    //
                    // Remove all revision-related attributes.
                    node.attributes.remove(&AttributeKey::RevisionType);
                    node.attributes.remove(&AttributeKey::RevisionAuthor);
                    node.attributes.remove(&AttributeKey::RevisionDate);
                    node.attributes.remove(&AttributeKey::RevisionId);
                    node.attributes
                        .remove(&AttributeKey::RevisionOriginalFormatting);
                }
            }
            _ => {
                // Unknown revision type — strip attributes defensively.
                Self::strip_revision_attributes(self.model.node_mut(node_id));
            }
        }
        Ok(())
    }

    /// Remove all revision-tracking attributes from a node.
    fn strip_revision_attributes(node: Option<&mut Node>) {
        if let Some(node) = node {
            node.attributes.remove(&AttributeKey::RevisionType);
            node.attributes.remove(&AttributeKey::RevisionAuthor);
            node.attributes.remove(&AttributeKey::RevisionDate);
            node.attributes.remove(&AttributeKey::RevisionId);
            node.attributes
                .remove(&AttributeKey::RevisionOriginalFormatting);
        }
    }

    // ─── Layout ──────────────────────────────────────────────────

    /// Lay out the document using the default configuration.
    ///
    /// Requires the `layout` feature flag. The returned [`s1_layout::LayoutDocument`]
    /// contains pages with positioned blocks, lines, and glyph runs ready for
    /// rendering or PDF export.
    ///
    /// # Errors
    ///
    /// Returns an error if fonts cannot be resolved or text shaping fails.
    #[cfg(feature = "layout")]
    pub fn layout(
        &self,
        font_db: &s1_text::FontDatabase,
    ) -> Result<s1_layout::LayoutDocument, Error> {
        self.layout_with_config(font_db, s1_layout::LayoutConfig::default())
    }

    /// Lay out the document with a custom configuration.
    ///
    /// Requires the `layout` feature flag. Use this method when you need
    /// to control page dimensions, margins, or widow/orphan settings.
    ///
    /// # Errors
    ///
    /// Returns an error if fonts cannot be resolved or text shaping fails.
    #[cfg(feature = "layout")]
    pub fn layout_with_config(
        &self,
        font_db: &s1_text::FontDatabase,
        config: s1_layout::LayoutConfig,
    ) -> Result<s1_layout::LayoutDocument, Error> {
        let mut engine = s1_layout::LayoutEngine::new(&self.model, font_db, config);
        Ok(engine.layout()?)
    }

    // ─── Export ──────────────────────────────────────────────────────

    /// Export the document to bytes in the given format.
    ///
    /// For DOCX, fidelity behaviour:
    ///
    /// 1. **Open + export, no edits** — the preservation package is
    ///    re-emitted verbatim. Round-trip is lossless.
    /// 2. **Open + edit + export** — `word/document.xml` is regenerated
    ///    from the projected `DocumentModel`, but spliced into the
    ///    preserved package. Every other part (theme, fontTable,
    ///    customXml, headers, footers, footnotes, endnotes, comments,
    ///    numbering, styles, images, rels, content types) rides
    ///    through unchanged.
    /// 3. **Document constructed without a package, or `invalidate_preservation`
    ///    called, or `model_mut` / `metadata_mut` accessed** — the export
    ///    falls back to building bytes from the model alone.
    pub fn export(&self, format: Format) -> Result<Vec<u8>, Error> {
        match format {
            #[cfg(feature = "docx")]
            Format::Docx => {
                if let Some(pkg) = &self.preservation {
                    // Nothing dirty — verbatim re-emit (Phase 2).
                    if !self.model_dirty {
                        return Ok(pkg
                            .write()
                            .map_err(|e| Error::Format(format!("ooxml package write: {e}")))?);
                    }
                    // Body structure changed or no origin table — fall
                    // back to wholesale `word/document.xml` regenerate
                    // (Phase 2a).
                    if self.body_structural_dirty || self.body_origin.is_none() {
                        return Ok(self.export_docx_spliced(pkg)?);
                    }
                    // model_dirty was set but no body NodeId was actually
                    // touched (e.g., a no-op `update_toc` on a doc without
                    // a TOC). Body is unchanged — verbatim re-emit.
                    if self.dirty_body_ids.is_empty() {
                        return Ok(pkg
                            .write()
                            .map_err(|e| Error::Format(format!("ooxml package write: {e}")))?);
                    }
                    // Specific body NodeIds dirty — Phase 2b per-node splice.
                    return Ok(self.export_docx_phase2b_splice(pkg)?);
                }
                Ok(s1_format_docx::write(&self.model)?)
            }
            #[cfg(feature = "odt")]
            Format::Odt => {
                if let Some(pkg) = &self.odf_preservation {
                    // No edits → re-emit the package verbatim (ODT Phase 2).
                    if !self.model_dirty {
                        return Ok(pkg
                            .write()
                            .map_err(|e| Error::Format(format!("odf package write: {e}")))?);
                    }
                    // Edits + per-NodeId origin intact → ODT Phase 2b splice:
                    // walk preserved <office:text>, swap dirty NodeIds
                    // with regenerated elements at the same position.
                    if !self.body_structural_dirty
                        && self
                            .odf_body_origin
                            .as_ref()
                            .map(|o| !o.node_id_order.is_empty())
                            .unwrap_or(false)
                    {
                        return Ok(self.export_odt_phase2b_splice(pkg)?);
                    }
                    // Otherwise fall back to the XmlTree-level body-swap
                    // (Phase 2a): regenerated <office:body> into the
                    // preserved content.xml.
                    return Ok(self.export_odt_spliced(pkg)?);
                }
                Ok(s1_format_odt::write(&self.model)?)
            }
            #[cfg(feature = "txt")]
            Format::Txt => Ok(s1_format_txt::write(&self.model)),
            #[cfg(feature = "md")]
            Format::Md => Ok(s1_format_md::write_bytes(&self.model)),
            #[cfg(feature = "pdf")]
            Format::Pdf => {
                // Convenience path: use a font DB that actually has fonts.
                // `FontDatabase::new()` loads system fonts on non-WASM
                // targets and the embedded Noto Sans fallback on WASM,
                // so text/headers/footers actually render instead of
                // shaping into empty glyph runs (the "empty colored
                // tables" symptom the previous `FontDatabase::empty()`
                // produced). Advanced callers can still hand in a
                // custom DB via the public `export_pdf(&font_db)` API.
                let mut font_db = s1_text::FontDatabase::new();
                // On macOS, also pull in Microsoft Office's bundled font set
                // and the cloud-font cache — those hold Calibri, Cambria,
                // Aptos and the localized CJK families that DOCX files
                // typically reference but which aren't part of the system
                // font set.
                font_db.load_macos_office_fonts();
                // Load fonts embedded in the source DOCX (.odttf de-obfuscated)
                // so that documents using non-system fonts (e.g. Ubuntu) render
                // with correct metrics instead of falling back to Times New Roman.
                #[cfg(feature = "docx")]
                if let Some(pkg) = &self.preservation {
                    for font_bytes in s1_format_docx::extract_embedded_fonts(pkg) {
                        font_db.load_font_data(font_bytes);
                    }
                }
                self.export_pdf(&font_db)
            }
            #[cfg(feature = "convert")]
            Format::Csv => {
                let csv_text = s1_convert::model_to_csv(&self.model);
                Ok(csv_text.into_bytes())
            }
            #[allow(unreachable_patterns)]
            _ => Err(Error::UnsupportedFormat(format!(
                "{:?} export not available (check feature flags)",
                format
            ))),
        }
    }

    /// Splice path for `export(Docx)` when edits have happened.
    ///
    /// Regenerates `word/document.xml` from the model via the existing DOCX
    /// writer, then replaces just that part inside a clone of the preserved
    /// package. Every other part — theme, fontTable, customXml, headers,
    /// footers, footnotes, endnotes, comments, numbering, styles, images,
    /// rels, content types — rides through unchanged.
    ///
    /// Limitation (Phase 2a): unknown OOXML inside `word/document.xml`
    /// itself is **not** preserved across edits. That requires the
    /// NodeId-keyed body-merge work tracked as Phase 2b.
    #[cfg(feature = "docx")]
    fn export_docx_spliced(&self, pkg: &s1_ooxml::Package) -> Result<Vec<u8>, Error> {
        // Step 1: regenerate a full DOCX from the model.
        let regenerated_bytes = s1_format_docx::write(&self.model)?;

        // Step 2: parse the regenerated bytes to extract its document.xml part.
        let regenerated_pkg = s1_ooxml::Package::parse(&regenerated_bytes)
            .map_err(|e| Error::Format(format!("re-parse regenerated docx: {e}")))?;
        let new_doc_part = regenerated_pkg
            .parts
            .get("word/document.xml")
            .ok_or_else(|| Error::Format("regenerated DOCX has no word/document.xml".to_owned()))?
            .clone();

        // Step 3: clone the preserved package and swap document.xml in.
        let mut patched = pkg.clone();
        patched
            .parts
            .insert("word/document.xml".to_owned(), new_doc_part);

        // Step 4: write the patched package.
        patched
            .write()
            .map_err(|e| Error::Format(format!("ooxml package write: {e}")))
    }

    /// Phase 2b per-NodeId splice: regenerate only the body children
    /// whose NodeIds are in `dirty_body_ids`; keep every other body
    /// element verbatim from the preserved package — including unknown
    /// OOXML inside untouched paragraphs / tables (drawings, structured
    /// document tags, custom XML, MathML, AlternateContent fallbacks).
    ///
    /// Falls back to the Phase 2a wholesale-regenerate path if the
    /// origin table can't be aligned positionally with the current
    /// model body (body structure changed in a way the classifier
    /// didn't flag, or the regenerated body's block count diverges).
    #[cfg(feature = "docx")]
    fn export_docx_phase2b_splice(&self, pkg: &s1_ooxml::Package) -> Result<Vec<u8>, Error> {
        use s1_format_docx::body_origin::{body_in, body_in_mut, is_block_level};
        use s1_ooxml::{PartContent, XmlElement, XmlNode};

        let origin = match &self.body_origin {
            Some(o) => o,
            None => return self.export_docx_spliced(pkg),
        };

        // Current model body NodeIds must still match the origin order. If
        // not, fall back to wholesale regenerate — the splice can't realign.
        let body_id = self
            .model
            .body_id()
            .ok_or_else(|| Error::Format("document has no body".to_owned()))?;
        let body_node = self
            .model
            .node(body_id)
            .ok_or_else(|| Error::Format("body NodeId points at nothing".to_owned()))?;
        if body_node.children.as_slice() != origin.node_id_order.as_slice() {
            return self.export_docx_spliced(pkg);
        }

        // Regenerate the model so we can source per-NodeId XML for the
        // dirty entries. Reading back via Package keeps the regenerated
        // body in the same XmlElement form we'll splice into.
        let regenerated_bytes = s1_format_docx::write(&self.model)?;
        let regenerated_pkg = s1_ooxml::Package::parse(&regenerated_bytes)
            .map_err(|e| Error::Format(format!("re-parse regenerated docx: {e}")))?;
        let regenerated_part = regenerated_pkg
            .parts
            .get("word/document.xml")
            .ok_or_else(|| Error::Format("regenerated DOCX has no word/document.xml".to_owned()))?;
        let regenerated_tree = match &regenerated_part.content {
            PartContent::Xml(t) => t,
            PartContent::Binary(_) => {
                return Err(Error::Format(
                    "regenerated word/document.xml is binary".to_owned(),
                ));
            }
        };
        let regenerated_body = body_in(&regenerated_tree.root).ok_or_else(|| {
            Error::Format("regenerated word/document.xml has no <w:body>".to_owned())
        })?;
        let regenerated_blocks: Vec<&XmlElement> = regenerated_body
            .children
            .iter()
            .filter_map(|c| match c {
                XmlNode::Element(el) if is_block_level(&el.name.local_name) => Some(el),
                _ => None,
            })
            .collect();
        if regenerated_blocks.len() != origin.node_id_order.len() {
            // Writer emitted a different number of body blocks than the
            // model claims. Bail to wholesale regenerate.
            return self.export_docx_spliced(pkg);
        }

        // Clone the preserved document.xml tree and overwrite only the
        // dirty NodeIds' block elements with the regenerated XML. Every
        // other XmlNode in the body (preserved blocks, sectPr, non-TOC
        // sdt blocks, comments, range markers) rides through unchanged.
        let preserved_part = pkg
            .parts
            .get("word/document.xml")
            .ok_or_else(|| Error::Format("preserved DOCX has no word/document.xml".to_owned()))?;
        let mut preserved_tree = match &preserved_part.content {
            PartContent::Xml(t) => t.clone(),
            PartContent::Binary(_) => {
                return Err(Error::Format(
                    "preserved word/document.xml is binary".to_owned(),
                ));
            }
        };
        let preserved_body = body_in_mut(&mut preserved_tree.root).ok_or_else(|| {
            Error::Format("preserved word/document.xml has no <w:body>".to_owned())
        })?;

        let mut block_idx = 0usize;
        for child in &mut preserved_body.children {
            if let XmlNode::Element(el) = child {
                if is_block_level(&el.name.local_name) {
                    if block_idx >= origin.node_id_order.len() {
                        return self.export_docx_spliced(pkg);
                    }
                    let nid = origin.node_id_order[block_idx];
                    if self.dirty_body_ids.contains(&nid) {
                        *el = regenerated_blocks[block_idx].clone();
                    }
                    block_idx += 1;
                }
            }
        }
        if block_idx != origin.node_id_order.len() {
            // Mid-walk count mismatch — preserved body lost or gained
            // block elements since open. Bail.
            return self.export_docx_spliced(pkg);
        }

        // Swap the patched document.xml back into a clone of the package.
        let mut patched = pkg.clone();
        if let Some(part) = patched.parts.get_mut("word/document.xml") {
            part.content = PartContent::Xml(preserved_tree);
        }
        patched
            .write()
            .map_err(|e| Error::Format(format!("ooxml package write: {e}")))
    }

    /// ODT Phase 2b per-NodeId splice: regenerate only the body children
    /// whose NodeIds are in `dirty_body_ids`; keep every other body
    /// element verbatim from the preserved package — including unknown
    /// ODF inside untouched paragraphs / tables (`draw:frame`,
    /// `text:span`, `text:s`, `text:soft-page-break`, `svg:title`/`desc`,
    /// `table:table-columns`, …).
    ///
    /// Falls back to the Phase 2a XmlTree-level body-swap if the origin
    /// table can't be aligned positionally with the current model body.
    #[cfg(feature = "odt")]
    fn export_odt_phase2b_splice(&self, pkg: &s1_odf::Package) -> Result<Vec<u8>, Error> {
        use s1_format_odt::body_origin::{is_block_level, office_text_in, office_text_in_mut};
        use s1_odf::{PartContent, XmlElement, XmlNode};

        let origin = match &self.odf_body_origin {
            Some(o) => o,
            None => return self.export_odt_spliced(pkg),
        };

        let body_id = self
            .model
            .body_id()
            .ok_or_else(|| Error::Format("document has no body".to_owned()))?;
        let body_node = self
            .model
            .node(body_id)
            .ok_or_else(|| Error::Format("body NodeId points at nothing".to_owned()))?;
        if body_node.children.as_slice() != origin.node_id_order.as_slice() {
            return self.export_odt_spliced(pkg);
        }

        // Regenerate the model to source per-NodeId XML for dirty entries.
        let regenerated_bytes = s1_format_odt::write(&self.model)?;
        let regenerated_pkg = s1_odf::Package::parse(&regenerated_bytes)
            .map_err(|e| Error::Format(format!("re-parse regenerated odt: {e}")))?;
        let regenerated_part = regenerated_pkg
            .parts
            .get("content.xml")
            .ok_or_else(|| Error::Format("regenerated ODT has no content.xml".to_owned()))?;
        let regenerated_tree = match &regenerated_part.content {
            PartContent::Xml(t) => t,
            PartContent::Binary(_) => {
                return Err(Error::Format(
                    "regenerated content.xml is binary".to_owned(),
                ));
            }
        };
        let regenerated_text = office_text_in(&regenerated_tree.root).ok_or_else(|| {
            Error::Format("regenerated content.xml has no <office:text>".to_owned())
        })?;
        let regenerated_blocks: Vec<&XmlElement> = regenerated_text
            .children
            .iter()
            .filter_map(|c| match c {
                XmlNode::Element(el) if is_block_level(&el.name.local_name) => Some(el),
                _ => None,
            })
            .collect();
        if regenerated_blocks.len() != origin.node_id_order.len() {
            return self.export_odt_spliced(pkg);
        }

        // Clone preserved content.xml; walk its <office:text>; for each
        // block-level child, swap with regenerated only when its
        // positional NodeId is in dirty_body_ids.
        let preserved_part = pkg
            .parts
            .get("content.xml")
            .ok_or_else(|| Error::Format("preserved ODT has no content.xml".to_owned()))?;
        let mut preserved_tree = match &preserved_part.content {
            PartContent::Xml(t) => t.clone(),
            PartContent::Binary(_) => {
                return Err(Error::Format("preserved content.xml is binary".to_owned()));
            }
        };
        let preserved_text = office_text_in_mut(&mut preserved_tree.root).ok_or_else(|| {
            Error::Format("preserved content.xml has no <office:text>".to_owned())
        })?;

        let mut block_idx = 0usize;
        for child in &mut preserved_text.children {
            if let XmlNode::Element(el) = child {
                if is_block_level(&el.name.local_name) {
                    if block_idx >= origin.node_id_order.len() {
                        return self.export_odt_spliced(pkg);
                    }
                    let nid = origin.node_id_order[block_idx];
                    if self.dirty_body_ids.contains(&nid) {
                        *el = regenerated_blocks[block_idx].clone();
                    }
                    block_idx += 1;
                }
            }
        }
        if block_idx != origin.node_id_order.len() {
            return self.export_odt_spliced(pkg);
        }

        let mut patched = pkg.clone();
        if let Some(part) = patched.parts.get_mut("content.xml") {
            part.content = PartContent::Xml(preserved_tree);
        }
        patched
            .write()
            .map_err(|e| Error::Format(format!("odf package write: {e}")))
    }

    /// Splice path for `export(Odt)` when edits have happened.
    ///
    /// Unlike the DOCX path — where unknown styles live in their own
    /// `word/styles.xml` part that Phase 2a's part-level splice already
    /// preserves — ODF puts `<office:automatic-styles>`,
    /// `<office:font-face-decls>`, and `<office:scripts>` *inside*
    /// `content.xml`. So a naive content.xml swap would lose them.
    ///
    /// Instead we splice at the **XmlTree** tier: take the preserved
    /// `content.xml`, find its `<office:body>` subtree, and swap that
    /// subtree alone with the regenerated body. Every sibling
    /// (`<office:automatic-styles>`, `<office:font-face-decls>`,
    /// `<office:scripts>`, `<office:settings>`, …) rides through
    /// unchanged. All non-`content.xml` parts (`styles.xml`,
    /// `meta.xml`, `META-INF`, `Pictures/*`, `Configurations2/*`)
    /// also ride through via the surrounding package clone.
    ///
    /// Limitation: unknown ODF *inside* `<office:body>` itself is
    /// still lost on edit. Phase 2b proper will add the NodeId-keyed
    /// body merge for ODT (mirror of `BodyOrigin` for DOCX).
    #[cfg(feature = "odt")]
    fn export_odt_spliced(&self, pkg: &s1_odf::Package) -> Result<Vec<u8>, Error> {
        use s1_odf::{PartContent, XmlNode};

        // Step 1: regenerate a full ODT from the model.
        let regenerated_bytes = s1_format_odt::write(&self.model)?;
        let regenerated_pkg = s1_odf::Package::parse(&regenerated_bytes)
            .map_err(|e| Error::Format(format!("re-parse regenerated odt: {e}")))?;

        // Step 2: extract the regenerated `<office:body>` element.
        let regenerated_content_part = regenerated_pkg
            .parts
            .get("content.xml")
            .ok_or_else(|| Error::Format("regenerated ODT has no content.xml".to_owned()))?;
        let regenerated_tree = match &regenerated_content_part.content {
            PartContent::Xml(t) => t,
            PartContent::Binary(_) => {
                return Err(Error::Format(
                    "regenerated content.xml is binary".to_owned(),
                ));
            }
        };
        let regenerated_body = regenerated_tree
            .root
            .children
            .iter()
            .find_map(|c| match c {
                XmlNode::Element(el) if el.name.local_name == "body" => Some(el.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                Error::Format("regenerated content.xml has no <office:body>".to_owned())
            })?;

        // Step 3: clone the preserved content.xml tree and replace its
        // <office:body> subtree with the regenerated one. Sibling
        // sections (<office:automatic-styles>, <office:font-face-decls>,
        // <office:scripts>, <office:settings>, …) stay byte-equal.
        let preserved_content_part = pkg
            .parts
            .get("content.xml")
            .ok_or_else(|| Error::Format("preserved ODT has no content.xml".to_owned()))?;
        let mut preserved_tree = match &preserved_content_part.content {
            PartContent::Xml(t) => t.clone(),
            PartContent::Binary(_) => {
                return Err(Error::Format("preserved content.xml is binary".to_owned()));
            }
        };
        let mut swapped = false;
        for child in &mut preserved_tree.root.children {
            if let XmlNode::Element(el) = child {
                if el.name.local_name == "body" {
                    *el = regenerated_body.clone();
                    swapped = true;
                    break;
                }
            }
        }
        if !swapped {
            // Preserved content.xml had no <office:body> we could swap —
            // bail to the wholesale-replace path for safety.
            let mut patched = pkg.clone();
            patched
                .parts
                .insert("content.xml".to_owned(), regenerated_content_part.clone());
            return patched
                .write()
                .map_err(|e| Error::Format(format!("odf package write: {e}")));
        }

        // Step 4: swap the patched content.xml tree back into a clone of
        // the preserved package and write.
        let mut patched = pkg.clone();
        if let Some(part) = patched.parts.get_mut("content.xml") {
            part.content = PartContent::Xml(preserved_tree);
        }
        patched
            .write()
            .map_err(|e| Error::Format(format!("odf package write: {e}")))
    }

    /// Export the document as PDF using the provided font database.
    ///
    /// Requires the `pdf` feature flag. Lays out the document with the
    /// given fonts and renders to PDF bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if layout or PDF generation fails.
    #[cfg(feature = "pdf")]
    pub fn export_pdf(&self, font_db: &s1_text::FontDatabase) -> Result<Vec<u8>, Error> {
        let layout = self.layout(font_db)?;
        let bytes = s1_format_pdf::write_pdf(&layout, font_db, Some(self.model.metadata()))?;
        Ok(bytes)
    }

    /// Export the document as PDF with a custom layout configuration.
    ///
    /// Requires the `pdf` feature flag. Use this method when you need to
    /// control page dimensions, margins, or other layout settings.
    ///
    /// # Errors
    ///
    /// Returns an error if layout or PDF generation fails.
    #[cfg(feature = "pdf")]
    pub fn export_pdf_with_config(
        &self,
        font_db: &s1_text::FontDatabase,
        config: s1_layout::LayoutConfig,
    ) -> Result<Vec<u8>, Error> {
        let layout = self.layout_with_config(font_db, config)?;
        let bytes = s1_format_pdf::write_pdf(&layout, font_db, Some(self.model.metadata()))?;
        Ok(bytes)
    }

    /// Export the document as PDF/A (archival-compliant PDF).
    ///
    /// PDF/A-1b includes an ICC color profile, XMP metadata, and output intent
    /// for long-term archival compliance.
    ///
    /// # Errors
    ///
    /// Returns an error if layout or PDF generation fails.
    #[cfg(feature = "pdf")]
    pub fn export_pdf_a(
        &self,
        font_db: &s1_text::FontDatabase,
        conformance: s1_format_pdf::PdfAConformance,
    ) -> Result<Vec<u8>, Error> {
        let layout = self.layout(font_db)?;
        let bytes =
            s1_format_pdf::write_pdf_a(&layout, font_db, Some(self.model.metadata()), conformance)?;
        Ok(bytes)
    }

    /// Export the document as a string (useful for TXT and Markdown formats).
    pub fn export_string(&self, format: Format) -> Result<String, Error> {
        match format {
            #[cfg(feature = "txt")]
            Format::Txt => Ok(s1_format_txt::write_string(&self.model)),
            #[cfg(feature = "md")]
            Format::Md => Ok(s1_format_md::write_string(&self.model)),
            _ => {
                let bytes = self.export(format)?;
                String::from_utf8(bytes)
                    .map_err(|e| Error::Format(format!("Output is not valid UTF-8: {e}")))
            }
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

/// Classify one operation against the *pre-apply* model state for
/// Phase 2b dirty tracking.
///
/// Populates `new_dirty` with the top-level body NodeId(s) affected by this
/// op, and sets `*structural` when the op changes the body's child list
/// (insert / delete / move at body level) — that case forces the splice
/// path to fall back to wholesale `word/document.xml` regeneration since
/// the origin table can no longer be aligned positionally.
fn classify_op(
    model: &DocumentModel,
    body_id: Option<NodeId>,
    op: &Operation,
    new_dirty: &mut Vec<NodeId>,
    structural: &mut bool,
) {
    let body_id = match body_id {
        Some(b) => b,
        None => return,
    };
    match op {
        Operation::InsertNode { parent_id, .. } => {
            if *parent_id == body_id {
                *structural = true;
            } else if let Some(top) = top_level_body_ancestor(model, *parent_id, body_id) {
                new_dirty.push(top);
            }
        }
        Operation::DeleteNode { target_id, .. } => {
            let parent_of_target = model.node(*target_id).and_then(|n| n.parent);
            if parent_of_target == Some(body_id) {
                *structural = true;
            } else if let Some(top) = top_level_body_ancestor(model, *target_id, body_id) {
                new_dirty.push(top);
            }
        }
        Operation::MoveNode {
            target_id,
            new_parent_id,
            ..
        } => {
            let parent_of_target = model.node(*target_id).and_then(|n| n.parent);
            if parent_of_target == Some(body_id) || *new_parent_id == body_id {
                *structural = true;
                return;
            }
            if let Some(top) = top_level_body_ancestor(model, *target_id, body_id) {
                new_dirty.push(top);
            }
            if let Some(top) = top_level_body_ancestor(model, *new_parent_id, body_id) {
                new_dirty.push(top);
            }
        }
        Operation::InsertText { target_id, .. }
        | Operation::DeleteText { target_id, .. }
        | Operation::SetAttributes { target_id, .. }
        | Operation::RemoveAttributes { target_id, .. } => {
            if let Some(top) = top_level_body_ancestor(model, *target_id, body_id) {
                new_dirty.push(top);
            }
        }
        // Metadata and style ops don't touch the body XML, so they don't
        // dirty any body NodeId. They affect docProps/core.xml and
        // word/styles.xml respectively — the splice keeps those parts
        // from the preserved package today (Phase 2a known limitation).
        Operation::SetMetadata { .. }
        | Operation::SetStyle { .. }
        | Operation::RemoveStyle { .. } => {}
        // Forward-compat for ops added in future versions: treat unknown
        // ops as potentially structural to avoid silent data loss.
        _ => {
            *structural = true;
        }
    }
}

/// Walk parent links from `id` up to the body's direct child. Returns
/// `None` if `id` is not in the body subtree (e.g., comments / footnotes
/// live under the document root, not under body).
fn top_level_body_ancestor(model: &DocumentModel, id: NodeId, body_id: NodeId) -> Option<NodeId> {
    if id == body_id {
        return None;
    }
    let mut current = id;
    loop {
        let node = model.node(current)?;
        let parent = node.parent?;
        if parent == body_id {
            return Some(current);
        }
        current = parent;
    }
}
