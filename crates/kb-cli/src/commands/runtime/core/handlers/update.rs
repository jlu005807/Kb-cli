pub(crate) fn run_update(
    dry_run: bool,
    prune: bool,
    full: bool,
    strict: bool,
    llm: bool,
) -> Result<Value, CliError> {
    let start = Instant::now();
    let cwd = ensure_initialized()?;
    // Load LLM runtime config at update-time so layered semantic stages can reuse it.
    let llm_runtime = resolve_llm_config(&cwd);
    let kb_doc = parse_kb_document_file(&cwd.join("kb.md"))?;
    let ignored = load_ignore_rules(&cwd).into_iter().collect::<BTreeSet<_>>();
    let desired = collect_desired_entries(&cwd, &kb_doc, &ignored, false, None)?;
    let mut registry = load_registry(&cwd)?;

    // 经由 StorageHandles 选择后端：fs 维持原行为，sqlite 让 L1-L5 全部走 SQL。
    // 打开失败（极少见——基本只有目录不可写）直接报到 CoreError，不再静默回退，
    // 防止用户配置 backend=sqlite 后却把数据继续写到 JSON。
    let storage = StorageHandles::open(&cwd).map_err(|err| {
        map_core_error(
            CoreErrorKind::PersistIndexFailed,
            json!({"reason": err.to_string()}),
        )
    })?;
    let index_store = &*storage.index;
    let knowledge_store = &*storage.knowledge;
    let search_store = &*storage.search;
    let graph_store = &*storage.graph;
    let wiki_store = &*storage.wiki;
    let content_reader = &storage.content_reader;

    let effective_baseline = index_store
        .read_index()
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;

    let diff = compute_registry_diff(&cwd, &registry.regs, &desired, effective_baseline.as_ref());
    apply_registry_diff(&mut registry, &diff);

    let backend_sync = build_backend_sync_backend();
    let mut raw_category_created = false;
    if !dry_run {
        let kb_md = cwd.join("kb.md");
        let kb_doc_for_prune = parse_kb_document_file(&kb_md)?;
        prune_missing_refs_from_kb_md(&cwd, &kb_doc_for_prune, &diff)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, json!({})))?;
        let kb_doc_for_dedupe = parse_kb_document_file(&kb_md)?;
        dedupe_raw_refs_in_kb_md(&cwd, &kb_doc_for_dedupe)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, json!({})))?;
        let kb_doc_for_append = parse_kb_document_file(&kb_md)?;
        let had_raw_category = kb_doc_for_append.categories.iter().any(|c| c == "raw");
        let appended_raw = append_missing_raw_refs_to_kb_md(&cwd, &kb_doc_for_append, &desired)
            .map_err(|_| map_core_error(CoreErrorKind::WriteConfigFailed, json!({})))?;
        raw_category_created = appended_raw > 0 && !had_raw_category;
        backend_sync.sync(&build_backend_sync_items(&diff)).map_err(|_| {
            map_core_error(CoreErrorKind::PersistIndexFailed, json!({"target":"backend"}))
        })?;
        save_registry(&cwd, &registry)?;
    }

    let refs = regs_to_records(&cwd, &registry.regs, &ignored, false, None);
    let effective_prune = prune || !diff.to_remove.is_empty();
    let outcome = IndexEngine.update(
        effective_baseline.as_ref(),
        &refs,
        &IndexConfig::new(ignored, effective_prune, full),
    );

    if !dry_run {
        index_store.write_index(&outcome.snapshot).map_err(|_| {
            map_core_error(
                CoreErrorKind::PersistIndexFailed,
                json!({"targets":[".kb/index.json",".kb/index.errors.json",".kb/index.meta.json"]}),
            )
        })?;
    }

    let mut progressive_issues = collect_progressive_layer_issues();

    let semantic_llm_requested = llm || llm_runtime.semantic_enabled;
    let graph_llm_requested = llm || llm_runtime.graph_enabled;
    let wiki_llm_requested = llm || llm_runtime.wiki_enabled;

    let semantic_mode = if semantic_llm_requested {
        if llm_runtime.key.is_some() {
            "llm"
        } else {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L2_SEMANTIC,
                code: "W-L2-LLM-CONFIG-MISSING".to_string(),
                message: if llm {
                    "LLM semantic extraction requested by --llm but KB_LLM_KEY is missing"
                        .to_string()
                } else {
                    "LLM semantic extraction enabled but KB_LLM_KEY is missing".to_string()
                },
                retryable: false,
            });
            "rule"
        }
    } else {
        "rule"
    };
    let graph_mode = if graph_llm_requested {
        if llm_runtime.key.is_some() {
            "llm"
        } else {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L4_GRAPH,
                code: "W-L4-LLM-CONFIG-MISSING".to_string(),
                message: if llm {
                    "LLM graph enrichment requested by --llm but KB_LLM_KEY is missing".to_string()
                } else {
                    "LLM graph enrichment enabled but KB_LLM_KEY is missing".to_string()
                },
                retryable: false,
            });
            "rule"
        }
    } else {
        "rule"
    };
    let wiki_mode = if wiki_llm_requested {
        if llm_runtime.key.is_some() {
            "llm"
        } else {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L5_WIKI_VIEW,
                code: "W-L5-LLM-CONFIG-MISSING".to_string(),
                message: if llm {
                    "LLM wiki enrichment requested by --llm but KB_LLM_KEY is missing".to_string()
                } else {
                    "LLM wiki enrichment enabled but KB_LLM_KEY is missing".to_string()
                },
                retryable: false,
            });
            "rule"
        }
    } else {
        "rule"
    };

    let llm_semantic_config = LlmSemanticConfig {
        key: llm_runtime.key.clone(),
        model: llm_runtime.model.clone(),
        base_url: llm_runtime.base_url.clone(),
        workspace_root: Some(cwd.clone()),
        progress: Some(Arc::new(|line| emit_runtime_trace("kb-llm", &line))),
        semantic_flow: llm_runtime.semantic_flow.clone(),
        timeout_secs: llm_runtime.timeout_secs,
        temperature: llm_runtime.temperature,
        max_output_tokens: llm_runtime.max_output_tokens,
        analysis_max_output_tokens: llm_runtime.analysis_max_output_tokens,
        max_input_chars: llm_runtime.max_input_chars,
        max_chunks: llm_runtime.max_chunks,
        retry_max_attempts: llm_runtime.retry_max_attempts,
        retry_backoff_ms: llm_runtime.retry_backoff_ms,
        agent_loop_max_rounds: llm_runtime.agent_loop_max_rounds,
        agent_reflection_enabled: llm_runtime.agent_reflection_enabled,
        agent_min_chunk_coverage: llm_runtime.agent_min_chunk_coverage,
    };
    let extractor: Box<dyn SemanticExtractor> = if semantic_mode == "llm" {
        Box::new(LlmSemanticExtractor::new(llm_semantic_config.clone()))
    } else {
        Box::new(RuleBasedSemanticExtractor::new())
    };

    let baseline_knowledge = match knowledge_store.read_knowledge() {
        Ok(snapshot) => snapshot,
        Err(err) => {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L2_SEMANTIC,
                code: "W-L2-READ-SNAPSHOT-FAILED".to_string(),
                message: err.to_string(),
                retryable: true,
            });
            None
        }
    };

    let generated_at = generated_at_unix_string();
    let l2_outcome = compile_l2(
        baseline_knowledge.as_ref(),
        &outcome.snapshot,
        content_reader,
        extractor.as_ref(),
        &generated_at,
    );
    for issue in &l2_outcome.issues {
        progressive_issues.push(LayerIssue {
            layer: LAYER_L2_SEMANTIC,
            code: issue.code.clone(),
            message: issue.message.clone(),
            retryable: issue.retryable,
        });
    }

    if !dry_run {
        if let Err(err) = knowledge_store.write_knowledge(&l2_outcome.snapshot) {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L2_SEMANTIC,
                code: "W-L2-WRITE-SNAPSHOT-FAILED".to_string(),
                message: err.to_string(),
                retryable: true,
            });
        }
    }

    let baseline_search = match search_store.read_search() {
        Ok(snapshot) => snapshot,
        Err(err) => {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L3_PROJECTION,
                code: "W-L3-READ-SNAPSHOT-FAILED".to_string(),
                message: err.to_string(),
                retryable: true,
            });
            None
        }
    };
    let l3_outcome = compile_l3(
        baseline_search.as_ref(),
        &l2_outcome.snapshot,
        &generated_at,
    );

    if !dry_run {
        if let Err(err) = search_store.write_search(&l3_outcome.snapshot) {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L3_PROJECTION,
                code: "W-L3-WRITE-SNAPSHOT-FAILED".to_string(),
                message: err.to_string(),
                retryable: true,
            });
        }
    }

    let baseline_graph = match graph_store.read_graph() {
        Ok(snapshot) => snapshot,
        Err(err) => {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L4_GRAPH,
                code: "W-L4-READ-SNAPSHOT-FAILED".to_string(),
                message: err.to_string(),
                retryable: true,
            });
            None
        }
    };
    let l4_rule_outcome = compile_l4(
        baseline_graph.as_ref(),
        &l2_outcome.snapshot,
        &generated_at,
    );
    let mut l4_metrics_json = Value::Null;
    let l4_outcome = if graph_mode == "llm" {
        let mut graph_trace = |line: &str| emit_runtime_trace("kb-llm", line);
        match enrich_graph_with_llm(
            baseline_graph.as_ref(),
            &l4_rule_outcome,
            &l2_outcome.snapshot,
            &llm_semantic_config,
            Some(&mut graph_trace),
        ) {
            Ok(enriched) => {
                l4_metrics_json = json!({
                    "map_reduce": {
                        "batch_size": enriched.metrics.map_reduce.batch_size,
                        "batches_total": enriched.metrics.map_reduce.batches_total,
                        "calls_total": enriched.metrics.map_reduce.calls_total,
                        "calls_succeeded": enriched.metrics.map_reduce.calls_succeeded,
                        "calls_failed": enriched.metrics.map_reduce.calls_failed,
                        "rounds_used": enriched.metrics.map_reduce.rounds_used,
                        "rounds_max": enriched.metrics.map_reduce.rounds_max,
                        "reflection_enabled": enriched.metrics.map_reduce.reflection_enabled,
                    },
                    "coverage": {
                        "covered": enriched.metrics.coverage.covered,
                        "total": enriched.metrics.coverage.total,
                        "ratio": enriched.metrics.coverage.ratio,
                        "target_ratio": enriched.metrics.coverage.target_ratio,
                    }
                });
                enriched.outcome
            }
            Err(err) => {
                progressive_issues.push(LayerIssue {
                    layer: LAYER_L4_GRAPH,
                    code: "W-L4-LLM-ENRICH-FAILED".to_string(),
                    message: err.to_string(),
                    retryable: true,
                });
                l4_rule_outcome
            }
        }
    } else {
        l4_rule_outcome
    };
    for issue in &l4_outcome.issues {
        progressive_issues.push(LayerIssue {
            layer: LAYER_L4_GRAPH,
            code: issue.code.clone(),
            message: issue.message.clone(),
            retryable: issue.retryable,
        });
    }
    if !dry_run {
        if let Err(err) = graph_store.write_graph(&l4_outcome.snapshot) {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L4_GRAPH,
                code: "W-L4-WRITE-SNAPSHOT-FAILED".to_string(),
                message: err.to_string(),
                retryable: true,
            });
        }
    }

    let baseline_wiki_manifest = match wiki_store.read_wiki_manifest() {
        Ok(snapshot) => snapshot,
        Err(err) => {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L5_WIKI_VIEW,
                code: "W-L5-READ-MANIFEST-FAILED".to_string(),
                message: err.to_string(),
                retryable: true,
            });
            None
        }
    };
    let l5_rule_outcome = compile_l5(
        baseline_wiki_manifest.as_ref(),
        &l2_outcome.snapshot,
        &l4_outcome.snapshot,
        &generated_at,
    );
    let mut l5_metrics_json = Value::Null;
    let l5_outcome = if wiki_mode == "llm" {
        let mut wiki_trace = |line: &str| emit_runtime_trace("kb-llm", line);
        match enrich_wiki_with_llm(
            baseline_wiki_manifest.as_ref(),
            &l5_rule_outcome,
            &l2_outcome.snapshot,
            &l4_outcome.snapshot,
            &generated_at,
            &llm_semantic_config,
            Some(&mut wiki_trace),
        ) {
            Ok(enriched) => {
                l5_metrics_json = json!({
                    "map_reduce": {
                        "batch_size": enriched.metrics.map_reduce.batch_size,
                        "batches_total": enriched.metrics.map_reduce.batches_total,
                        "calls_total": enriched.metrics.map_reduce.calls_total,
                        "calls_succeeded": enriched.metrics.map_reduce.calls_succeeded,
                        "calls_failed": enriched.metrics.map_reduce.calls_failed,
                        "rounds_used": enriched.metrics.map_reduce.rounds_used,
                        "rounds_max": enriched.metrics.map_reduce.rounds_max,
                        "reflection_enabled": enriched.metrics.map_reduce.reflection_enabled,
                    },
                    "coverage": {
                        "covered": enriched.metrics.coverage.covered,
                        "total": enriched.metrics.coverage.total,
                        "ratio": enriched.metrics.coverage.ratio,
                        "target_ratio": enriched.metrics.coverage.target_ratio,
                    }
                });
                enriched.outcome
            }
            Err(err) => {
                progressive_issues.push(LayerIssue {
                    layer: LAYER_L5_WIKI_VIEW,
                    code: "W-L5-LLM-ENRICH-FAILED".to_string(),
                    message: err.to_string(),
                    retryable: true,
                });
                l5_rule_outcome
            }
        }
    } else {
        l5_rule_outcome
    };
    for issue in &l5_outcome.issues {
        progressive_issues.push(LayerIssue {
            layer: LAYER_L5_WIKI_VIEW,
            code: issue.code.clone(),
            message: issue.message.clone(),
            retryable: issue.retryable,
        });
    }
    if !dry_run {
        if let Err(err) = wiki_store.write_wiki_view(&l5_outcome.snapshot, &l5_outcome.removed_paths)
        {
            progressive_issues.push(LayerIssue {
                layer: LAYER_L5_WIKI_VIEW,
                code: "W-L5-WRITE-VIEW-FAILED".to_string(),
                message: err.to_string(),
                retryable: true,
            });
        }
    }

    let mut l2_status = "ok";
    let mut l3_status = "ok";
    let mut l4_status = "ok";
    let mut l5_status = "ok";
    if progressive_issues
        .iter()
        .any(|issue| issue.layer == LAYER_L2_SEMANTIC)
    {
        l2_status = if strict { "error" } else { "warning" };
    }
    if progressive_issues.iter().any(|issue| issue.layer == LAYER_L3_PROJECTION) {
        l3_status = if strict { "error" } else { "warning" };
    }
    if progressive_issues.iter().any(|issue| issue.layer == LAYER_L4_GRAPH) {
        l4_status = if strict { "error" } else { "warning" };
    }
    if progressive_issues
        .iter()
        .any(|issue| issue.layer == LAYER_L5_WIKI_VIEW)
    {
        l5_status = if strict { "error" } else { "warning" };
    }

    let layer_status = json!({
        LAYER_L0_DECLARATION: "ok",
        LAYER_L1_REGISTRY: "ok",
        LAYER_L1_INDEX: "ok",
        LAYER_L2_SEMANTIC: l2_status,
        LAYER_L3_PROJECTION: l3_status,
        LAYER_L4_GRAPH: l4_status,
        LAYER_L5_WIKI_VIEW: l5_status,
    });
    let l2_compilation = json!({
        "processed": l2_outcome.processed,
        "reused": l2_outcome.reused,
        "missing": l2_outcome.missing,
        "failed": l2_outcome.failed,
    });
    let warning_items: Vec<Value> = progressive_issues.iter().map(issue_json).collect();
    let warning_texts: Vec<String> = progressive_issues.iter().map(issue_text).collect();

    if strict && !progressive_issues.is_empty() {
        return Err(map_core_error(
            CoreErrorKind::StrictLayerFailure,
            json!({
                "strict": true,
                "layer_status": layer_status,
                "l2_compilation": l2_compilation,
                "warnings": [],
                "errors": warning_items,
                "failed_layers": progressive_issues.iter().map(|issue| issue.layer).collect::<Vec<_>>(),
            }),
        ));
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let text = format_update_text(&UpdateTextSummary {
        added: diff.add_entries.len(),
        updated: diff.to_update.len() + outcome.updated,
        removed: diff.removed_paths.len(),
        marked_invalid: outcome.marked_invalid,
        duration_ms,
        dry_run,
        prune: effective_prune,
        full,
        strict,
        layer_status: vec![
            (LAYER_L0_DECLARATION, "ok"),
            (LAYER_L1_REGISTRY, "ok"),
            (LAYER_L1_INDEX, "ok"),
            (LAYER_L2_SEMANTIC, l2_status),
            (LAYER_L3_PROJECTION, l3_status),
            (LAYER_L4_GRAPH, l4_status),
            (LAYER_L5_WIKI_VIEW, l5_status),
        ],
        l2_processed: l2_outcome.processed,
        l2_reused: l2_outcome.reused,
        l2_missing: l2_outcome.missing,
        l2_failed: l2_outcome.failed,
        warnings: warning_texts,
        errors: vec![],
    });

    let data = json!({
        "added": diff.add_entries.len(),
        "updated": diff.to_update.len() + outcome.updated,
        "removed": diff.removed_paths.len(),
        "marked_invalid": outcome.marked_invalid,
        "duration_ms": duration_ms,
        "full": full,
        "raw_category_created": raw_category_created,
        "strict": strict,
        "layer_status": layer_status,
        "l2_compilation": l2_compilation,
        "semantic_extractor": {
            "mode": semantic_mode,
            "forced_by_cli": llm,
            "provider": if semantic_mode == "llm" { Some(format!("openai-compatible:{}@{}", llm_runtime.model, llm_runtime.base_url)) } else { None },
            "semantic_flow": llm_runtime.semantic_flow,
            "agent_tools": {
                "enabled": semantic_mode == "llm",
                "tools": [
                    "read_raw_file",
                    "read_knowledge_graph",
                    "read_file_summary"
                ]
            },
            "temperature": llm_runtime.temperature,
            "max_output_tokens": llm_runtime.max_output_tokens,
            "analysis_max_output_tokens": llm_runtime.analysis_max_output_tokens,
            "max_input_chars": llm_runtime.max_input_chars,
            "max_chunks": llm_runtime.max_chunks,
            "timeout_secs": llm_runtime.timeout_secs,
            "retry_max_attempts": llm_runtime.retry_max_attempts,
            "retry_backoff_ms": llm_runtime.retry_backoff_ms,
            "agent_loop_max_rounds": llm_runtime.agent_loop_max_rounds,
            "agent_reflection_enabled": llm_runtime.agent_reflection_enabled,
            "agent_min_chunk_coverage": llm_runtime.agent_min_chunk_coverage
        },
        "graph_builder": {
            "mode": graph_mode,
            "provider": if graph_mode == "llm" { Some(format!("openai-compatible:{}@{}", llm_runtime.model, llm_runtime.base_url)) } else { None },
            "agent_tools": {
                "enabled": graph_mode == "llm",
                "tools": [
                    "read_file_summary",
                    "read_knowledge_graph",
                    "read_graph_neighbors"
                ]
            },
            "llm_metrics": l4_metrics_json
        },
        "wiki_builder": {
            "mode": wiki_mode,
            "provider": if wiki_mode == "llm" { Some(format!("openai-compatible:{}@{}", llm_runtime.model, llm_runtime.base_url)) } else { None },
            "agent_tools": {
                "enabled": wiki_mode == "llm",
                "tools": [
                    "read_file_summary",
                    "read_knowledge_graph",
                    "read_wiki_page"
                ]
            },
            "llm_metrics": l5_metrics_json
        },
        "warnings": warning_items,
        "errors": [],
    });
    Ok(with_text(data, text))
}
