# backend.php -> TypeScript / Rust port ledger

Source: [`backend.php`](../backend.php) — **20527 lines, 266 `fd_*` functions, 57 route patterns**.

> **Rust column is superseded.** It was pre-filled with `N/A: Rust port not in
scope` before the port existed. The real per-group state is tracked in
> [`server/PARITY.md`](rust/PARITY.md); both use the same group names and no
> row is dropped. The TypeScript column below is unchanged.

Rule: **every row must have a counterpart in both ports** (or an explicit `N/A + reason`).
Tick a box per port only when the counterpart exists AND was exercised at least once.

| Group            | Function                                | backend.php | lines | TS                                               | Rust                        |
| ---------------- | --------------------------------------- | ----------- | ----- | ------------------------------------------------ | --------------------------- |
| foundation       | `fd_env`                                | :15         | 49    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_get_storage_dir`                    | :87         | 62    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_storage_path`                       | :149        | 113   | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_cache_path`                         | :262        | 12    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_prune_cache_files`                  | :274        | 94    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_is_debug_enabled`                   | :368        | 32    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_set_debug_enabled`                  | :400        | 14    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_limit_file_size`                    | :414        | 34    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_json`                               | :448        | 32    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_log_stderr`                         | :480        | 8     | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_log`                                | :488        | 28    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_get_app_root`                       | :11128      | 18    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_get_listen_port`                    | :11146      | 13    | [x]                                              | N/A: Rust port not in scope |
| foundation       | `fd_is_windows`                         | :11159      | 20    | N/A: process.platform                            | N/A: Rust port not in scope |
| foundation       | `fd_can_spawn_process`                  | :11179      | 20    | N/A: child_process is always available           | N/A: Rust port not in scope |
| foundation       | `fd_get_device_id`                      | :11265      | 28    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_get_api_secret`                     | :516        | 13    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_save_api_secret`                    | :529        | 13    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_clear_api_secret`                   | :542        | 12    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_cache`                         | :1485       | 9     | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_load`                          | :1494       | 53    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_valid_tokens`                  | :1547       | 19    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_save`                          | :1566       | 8     | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_enabled`                       | :1574       | 5     | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_token`                         | :1579       | 5     | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_rotate_token`                  | :1584       | 8     | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_verify_password`               | :1592       | 19    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_set_password`                  | :1611       | 9     | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_client_ip`                     | :1620       | 20    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_lockout_check`                 | :1640       | 18    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_record_failure`                | :1658       | 42    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_record_success`                | :1700       | 17    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_token_from_request`            | :1717       | 31    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_is_authenticated`                   | :1748       | 20    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_require_auth`                       | :1768       | 10    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_strip_token_prefix`                 | :1778       | 15    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_stremio_locked_stream`              | :1793       | 14    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_eclipse_locked_response`            | :1807       | 12    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_serve_auth_gate_html`               | :1819       | 65    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| auth             | `fd_is_local_request`                   | :1312       | 63    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_require_local_request`              | :1375       | 15    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_is_loopback_request`                | :1390       | 25    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_auth_privileged`                    | :1415       | 20    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_require_privileged`                 | :1435       | 15    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_is_cloudflare_tunnel_request`       | :1262       | 26    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_is_https_request`                   | :1288       | 24    | [x]                                              | N/A: Rust port not in scope |
| auth             | `fd_url_host_is_public`                 | :1450       | 35    | [x]                                              | N/A: Rust port not in scope |
| http             | `fd_http_json`                          | :1901       | 50    | [x]                                              | N/A: Rust port not in scope |
| http             | `fd_cached_wp_json`                     | :1951       | 123   | [x]                                              | N/A: Rust port not in scope |
| http             | `fd_curl_resolve_entries`               | :2074       | 17    | N/A: no cURL                                     | N/A: Rust port not in scope |
| http             | `fd_resolve_host_ips`                   | :2091       | 36    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| http             | `fd_get_upstream_manifest_hosts`        | :2127       | 81    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| http             | `fd_get_amphp_client`                   | :2208       | 77    | N/A: no Amp; fetch() is the transport            | N/A: Rust port not in scope |
| http             | `fd_amphp_http_request`                 | :2285       | 78    | N/A: no Amp                                      | N/A: Rust port not in scope |
| http             | `fd_http_get_many_amp`                  | :2363       | 68    | N/A: no Amp                                      | N/A: Rust port not in scope |
| http             | `fd_fallback_url`                       | :2431       | 16    | [x]                                              | N/A: Rust port not in scope |
| http             | `fd_is_connection_reset`                | :2447       | 17    | [x]                                              | N/A: Rust port not in scope |
| http             | `fd_http_get_contents`                  | :2464       | 282   | [x]                                              | N/A: Rust port not in scope |
| http             | `fd_http_fire_and_forget`               | :3948       | 78    | [x]                                              | N/A: Rust port not in scope |
| http             | `fd_http_download_file`                 | :12667      | 125   | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_get_bot_id`                         | :554        | 45    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_clear_bot_id`                       | :599        | 8     | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_get_bot_session_path`               | :607        | 9     | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_has_local_session`                  | :616        | 24    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_load_session_meta`                  | :640        | 10    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_save_session_meta`                  | :650        | 28    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_clear_session_meta`                 | :678        | 10    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_get_bot_pool`                       | :688        | 37    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_save_bot_pool`                      | :725        | 13    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_add_pool_bot`                       | :738        | 29    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_remove_pool_bot`                    | :767        | 47    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_pick_pool_bot`                      | :814        | 45    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_is_guest_provision_in_progress`     | :859        | 57    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_spawn_guest_provision`              | :916        | 60    | N/A: provisioning is inline/awaited              | N/A: Rust port not in scope |
| telegram_session | `fd_auto_provision_guest`               | :976        | 286   | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_clear_session_directory`            | :7539       | 78    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_clear_session`                      | :7617       | 64    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_find_orphan_session_bot_id`         | :7347       | 50    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_decrypt_credentials`                | :6324       | 38    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_fetch_credentials_from_wordpress`   | :6362       | 59    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_load_cached_api_credentials`        | :6421       | 18    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_save_cached_api_credentials`        | :6439       | 21    | [x]                                              | N/A: Rust port not in scope |
| telegram_session | `fd_environment_preflight`              | :6245       | 79    | N/A: Node runtime has no PHP extension preflight | N/A: Rust port not in scope |
| madeline         | `fd_load_madeline_autoload`             | :6170       | 24    | N/A: replaced by teleproto                       | N/A: Rust port not in scope |
| madeline         | `fd_ensure_autoload`                    | :6194       | 15    | N/A: replaced by teleproto                       | N/A: Rust port not in scope |
| madeline         | `fd_require_fileinfo`                   | :6209       | 36    | N/A: no PHP extensions                           | N/A: Rust port not in scope |
| madeline         | `fd_ipc_worker_running`                 | :6460       | 51    | N/A: no IPC worker (in-process teleproto client) | N/A: Rust port not in scope |
| madeline         | `fd_ipc_worker_ready`                   | :6511       | 45    | N/A: no IPC worker                               | N/A: Rust port not in scope |
| madeline         | `fd_ipc_state_is_success`               | :6556       | 20    | N/A: no IPC worker                               | N/A: Rust port not in scope |
| madeline         | `fd_ipc_state_is_failed`                | :6576       | 33    | N/A: no IPC worker                               | N/A: Rust port not in scope |
| madeline         | `fd_ensure_ipc_worker`                  | :6609       | 167   | N/A: no IPC worker                               | N/A: Rust port not in scope |
| madeline         | `fd_prewarm_ipc_worker`                 | :6776       | 49    | N/A: no IPC worker                               | N/A: Rust port not in scope |
| madeline         | `fd_boot_madeline`                      | :6825       | 451   | N/A: src/telegram.ts bootTelegram()              | N/A: Rust port not in scope |
| madeline         | `fd_is_dpi_transport_error`             | :7276       | 45    | N/A: no obfuscation fallback needed yet          | N/A: Rust port not in scope |
| madeline         | `fd_is_clock_or_authkey_error`          | :7321       | 26    | [x]                                              | N/A: Rust port not in scope |
| madeline         | `fd_measure_clock_offset`               | :7397       | 59    | [x]                                              | N/A: Rust port not in scope |
| madeline         | `fd_clock_skew_message`                 | :7456       | 24    | [x]                                              | N/A: Rust port not in scope |
| madeline         | `fd_friendly_login_error`               | :7480       | 59    | [x]                                              | N/A: Rust port not in scope |
| resolve          | `fd_resolve_cache_store`                | :3397       | 21    | [x]                                              | N/A: Rust port not in scope |
| resolve          | `fd_resolve_shortcode_cached`           | :3418       | 28    | [x]                                              | N/A: Rust port not in scope |
| resolve          | `fd_save_resolve_cache`                 | :3446       | 8     | [x]                                              | N/A: Rust port not in scope |
| resolve          | `fd_invalidate_resolve_cache`           | :3454       | 25    | [x]                                              | N/A: Rust port not in scope |
| resolve          | `fd_resolve_shortcode_fresh`            | :3479       | 30    | [x]                                              | N/A: Rust port not in scope |
| resolve          | `fd_resolve_shortcode_concurrent`       | :3509       | 359   | [x]                                              | N/A: Rust port not in scope |
| resolve          | `fd_resolve_shortcode`                  | :3868       | 80    | [x]                                              | N/A: Rust port not in scope |
| resolve          | `fd_spawn_warmup_resolve`               | :4091       | 53    | N/A: warmup runs inline                          | N/A: Rust port not in scope |
| resolve          | `fd_resolve_shortcodes_batch`           | :4196       | 49    | [x]                                              | N/A: Rust port not in scope |
| resolve          | `fd_warmup_resolve_batch`               | :4245       | 177   | [x]                                              | N/A: Rust port not in scope |
| resolve          | `fd_prewarm_streams_batch`              | :4422       | 49    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| resolve          | `fd_decode_download_payload`            | :1884       | 17    | [x]                                              | N/A: Rust port not in scope |
| matching         | `fd_format_bytes`                       | :7804       | 11    | [x]                                              | N/A: Rust port not in scope |
| matching         | `fd_clean_html_entities`                | :7815       | 21    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_build_search_query_variants`        | :7836       | 61    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_clean_media_title`                  | :7897       | 29    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_extract_media_tags`                 | :7926       | 158   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_calculate_stream_sort_score`        | :8084       | 60    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_extract_split_part_info`            | :8144       | 71    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_group_split_parts`                  | :8215       | 61    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_clean_post_title`                   | :8276       | 11    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_clean_post_plot`                    | :8287       | 11    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_extract_release_year`               | :8298       | 16    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_extract_post_genres`                | :8314       | 40    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_classify_season_episode`            | :8354       | 144   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_file_matches_episode`               | :8498       | 23    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_is_series_file`                     | :8521       | 48    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_normalize_episode_name`             | :8569       | 12    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_filename_episode_subtitle`          | :8581       | 55    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_episode_meta_identity_matches`      | :8636       | 32    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_extract_episode_names`              | :8668       | 35    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_public_episode_meta_json`           | :8703       | 44    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_upstream_episode_names`             | :8747       | 103   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_is_release_annotation`              | :8850       | 13    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_episode_subtitle_matches`           | :8863       | 33    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_movie_file_matches_title`           | :8896       | 324   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_series_file_matches_title`          | :9220       | 233   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_strip_title_decorations`            | :4957       | 24    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_norm_title_for_match`               | :4981       | 16    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_fold_latin_accents`                 | :4997       | 238   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_title_words_prefix_match`           | :5235       | 40    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| matching         | `fd_is_topkeyword_valid`                | :11655      | 88    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_lookup_local_catalog_by_prefix`     | :2746       | 55    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_media_ids_upsert`                   | :2801       | 75    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_media_ids_delete`                   | :2876       | 33    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_upstream_manifest_json`             | :2909       | 40    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_manifest_has_resource`              | :2949       | 30    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_upstream_manifests_with_resource`   | :2979       | 36    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_get_upstream_resolver_manifests`    | :3015       | 26    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_resolve_external_media_metadata`    | :3041       | 356   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_lookup_catalog_post_identity`       | :4700       | 87    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_lookup_imdb_id_by_title`            | :4787       | 56    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_search_upstream_for_imdb`           | :4843       | 114   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_media_ids_write`                    | :5275       | 59    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_persist_imdb_id_for_post`           | :5334       | 70    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_parse_imdb_aka_markdown`            | :5404       | 37    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_pick_imdb_aka`                      | :5441       | 33    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_jina_api_key`                       | :5474       | 44    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_aka_from_imdb_releaseinfo`          | :5518       | 132   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_aka_from_upstream`                  | :5650       | 198   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_persist_aka_for_imdb`               | :5848       | 34    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_imdb_id_for_catalog_post`           | :5882       | 47    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_search_own_imdb_api_for_tt`         | :5929       | 118   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_find_imdb_id_for_title`             | :6047       | 123   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_stream_ajax_ttl`                    | :9453       | 32    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_fetch_stream_ajax`                  | :9485       | 115   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_manifest_categories_cached`         | :9600       | 71    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_fetch_post_files_paged`             | :9671       | 134   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_stream_keyword_from_post_title`     | :9805       | 15    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_stream_keyword_variants`            | :9820       | 28    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_episode_stream_filter`              | :9848       | 33    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_search_files_parallel`              | :9881       | 106   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_fetch_episode_stream_files`         | :9987       | 411   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_fetch_series_episode_files`         | :10398      | 108   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_get_trending_keywords_map`          | :11793      | 98    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_load_catalog_settings`              | :11891      | 235   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_save_catalog_settings`              | :12126      | 13    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_get_default_catalog_options`        | :11743      | 50    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| wp_data          | `fd_detect_country`                     | :12139      | 46    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| stremio          | `fd_stremio_json`                       | :13547      | 21    | [x]                                              | N/A: Rust port not in scope |
| stremio          | `fd_stremio_manifest_identity`          | :10831      | 123   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| stremio          | `fd_get_stremio_base_url`               | :10954      | 40    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| stremio          | `fd_guess_video_mime`                   | :10994      | 29    | [x]                                              | N/A: Rust port not in scope |
| stremio          | `fd_mime_to_extension`                  | :11023      | 35    | [x]                                              | N/A: Rust port not in scope |
| stremio          | `fd_stremio_stream_filename`            | :11058      | 38    | [x]                                              | N/A: Rust port not in scope |
| stremio          | `fd_build_stremio_stream_url`           | :11096      | 11    | [x]                                              | N/A: Rust port not in scope |
| stremio          | `fd_is_public_download_path`            | :11107      | 5     | [x]                                              | N/A: Rust port not in scope |
| stremio          | `fd_extract_download_payload_from_path` | :11112      | 16    | [x]                                              | N/A: Rust port not in scope |
| eclipse          | `fd_eclipse_parse_audio_title`          | :13568      | 265   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| eclipse          | `fd_eclipse_format_track`               | :13833      | 213   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| eclipse          | `fd_eclipse_audio_quality_score`        | :14046      | 6481  | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| network          | `fd_is_usable_lan_ipv4`                 | :10506      | 35    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| network          | `fd_is_skipped_lan_iface`               | :10541      | 35    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| network          | `fd_lan_iface_score`                    | :10576      | 21    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| network          | `fd_pick_lan_ip_from_text`              | :10597      | 35    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| network          | `fd_lan_ip_from_php_ifaces`             | :10632      | 35    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| network          | `fd_is_android_runtime`                 | :10667      | 11    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| network          | `fd_cached_lan_ip`                      | :10678      | 27    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| network          | `fd_get_lan_ip_fast`                    | :10705      | 17    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| network          | `fd_get_lan_ip`                         | :10722      | 72    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| network          | `fd_get_live_tunnel_https_origin`       | :10794      | 37    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_dir`                         | :11199      | 9     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_token_path`                  | :11208      | 5     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_load_saved_tunnel_token`            | :11213      | 9     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_save_tunnel_token`                  | :11222      | 8     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_state_path`                  | :11230      | 5     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_pid_path`                    | :11235      | 5     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_log_path`                    | :11240      | 5     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_err_log_path`                | :11245      | 5     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_config_path`                 | :11250      | 5     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_bin_dir`                     | :11255      | 10    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_device_id`                   | :12185      | 5     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_subdomain`                   | :12190      | 5     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_register_worker`             | :12195      | 36    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_register_worker_async`       | :12231      | 12    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_load_tunnel_state`                  | :12243      | 22    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_save_tunnel_state`                  | :12265      | 17    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_clear_tunnel_state`                 | :12282      | 18    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_read_pid`                    | :12300      | 10    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_write_pid`                   | :12310      | 5     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_pid_alive`                   | :12315      | 27    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_kill_pid`                    | :12342      | 25    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_kill_leftovers`              | :12367      | 26    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_normalize_url`               | :12393      | 19    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_parse_url_from_text`         | :12412      | 15    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_read_logs`                   | :12427      | 11    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_parse_ingress_domains`       | :12438      | 35    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_pick_metrics_port`           | :12473      | 16    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_local_http_get`              | :12489      | 54    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_metrics_ports`               | :12543      | 43    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_parse_quicktunnel_body`      | :12586      | 23    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_read_quicktunnel_url`        | :12609      | 16    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_asset_name`                  | :12625      | 19    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_bin_path`                    | :12644      | 6     | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_which`                       | :12650      | 17    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_extract_tgz`                 | :12792      | 36    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_ensure_cloudflared`                 | :12828      | 49    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_write_dummy_config`          | :12877      | 16    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_spawn`                       | :12893      | 237   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_wait_for_url`                | :13130      | 31    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_auto_restart`                | :13161      | 88    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_tunnel_extract_token`               | :13249      | 13    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_get_tunnel_status`                  | :13262      | 125   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_disable_tunnel`                     | :13387      | 18    | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| tunnel           | `fd_enable_tunnel`                      | :13405      | 142   | [ ] not ported (see README gap list)             | N/A: Rust port not in scope |
| diagnostics      | `fd_diagnostics_settings_path`          | :11293      | 5     | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_diagnostics_settings`               | :11298      | 11    | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_diagnostics_auto_report_enabled`    | :11309      | 5     | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_diagnostics_set_auto_report`        | :11314      | 11    | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_reports_dir`                        | :11325      | 14    | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_device_info`                        | :11339      | 22    | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_diag_prune`                         | :11361      | 19    | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_diag_record`                        | :11380      | 38    | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_diag_report_send`                   | :11418      | 26    | N/A: uploader not ported                         | N/A: Rust port not in scope |
| diagnostics      | `fd_diag_flush_pending`                 | :11444      | 21    | N/A: uploader not ported                         | N/A: Rust port not in scope |
| diagnostics      | `fd_diag_list`                          | :11465      | 23    | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_diag_counts`                        | :11488      | 17    | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_diag_clear`                         | :11505      | 12    | [x]                                              | N/A: Rust port not in scope |
| diagnostics      | `fd_diag_install_fatal_handlers`        | :11517      | 138   | N/A: process-level handlers in server.ts         | N/A: Rust port not in scope |
| version          | `fd_update_version_state`               | :7681       | 38    | N/A: no on-disk version state                    | N/A: Rust port not in scope |
| version          | `fd_check_version`                      | :7719       | 85    | [x]                                              | N/A: Rust port not in scope |

## Ungrouped functions (still must be ported)

- [ ] `fd_is_temp_app_dir` — :64 (13 lines)
- [ ] `fd_storage_has_session` — :77 (10 lines)
- [ ] `fd_prewarm_stream_list` — :4026 (65 lines)
- [ ] `fd_spawn_backend_cli` — :4144 (52 lines)
- [ ] `fd_get_item_subtitles` — :4471 (106 lines)
- [ ] `fd_resolve_subtitle_target_id` — :4577 (79 lines)
- [ ] `fd_shortcode_display_title` — :4656 (44 lines)

## Routes / dispatch patterns

| Pattern                        | backend.php lines   | TS     | Rust         |
| ------------------------------ | ------------------- | ------ | ------------ | ----- | --- | --- |
| `/`                            | 20444, 20454, 20498 | [ ]    | [ ]          |
| `/api/*`                       | 18279               | [ ]    | [ ]          |
| `/api/auth/login`              | 18381               | [ ]    | [ ]          |
| `/api/auth/logout`             | 18429               | [ ]    | [ ]          |
| `/api/auth/password`           | 18434               | [ ]    | [ ]          |
| `/api/auth/reset`              | 18455               | [ ]    | [ ]          |
| `/api/auth/status`             | 18369               | [ ]    | [ ]          |
| `/api/auth/token/rotate`       | 18448               | [ ]    | [ ]          |
| `/api/botlogin`                | 19416               | [ ]    | [ ]          |
| `/api/botlogout`               | 19556               | [ ]    | [ ]          |
| `/api/bots`                    | 18928               | [ ]    | [ ]          |
| `/api/bots/add`                | 18949               | [ ]    | [ ]          |
| `/api/bots/remove`             | 19059               | [ ]    | [ ]          |
| `/api/bots/set-active`         | 19078               | [ ]    | [ ]          |
| `/api/catalog-settings`        | 18496, 18507        | [ ]    | [ ]          |
| `/api/clock-check`             | 18868               | [ ]    | [ ]          |
| `/api/country`                 | 18755, 18787        | [ ]    | [ ]          |
| `/api/debug-mode`              | 19206               | [ ]    | [ ]          |
| `/api/diagnostics`             | 19149               | [ ]    | [ ]          |
| `/api/diagnostics/settings`    | 19158               | [ ]    | [ ]          |
| `/api/download`                | 11109               | [ ]    | [ ]          |
| `/api/download/*`              | 11109               | [ ]    | [ ]          |
| `/api/lan-ip`                  | 18469               | [ ]    | [ ]          |
| `/api/logs`                    | 19225               | [ ]    | [ ]          |
| `/api/media-ids`               | 18668               | [ ]    | [ ]          |
| `/api/provision`               | 18888               | [ ]    | [ ]          |
| `/api/proxy-stream`            | 19577               | [ ]    | [ ]          |
| `/api/reports`                 | 19172               | [ ]    | [ ]          |
| `/api/resolve-shortcode`       | 19313               | [ ]    | [ ]          |
| `/api/session`                 | 18804               | [ ]    | [ ]          |
| `/api/sub-proxy`               | 18321               | [ ]    | [ ]          |
| `/api/tunnel/disable`          | 18490               | [ ]    | [ ]          |
| `/api/tunnel/enable`           | 18481               | [ ]    | [ ]          |
| `/api/tunnel/status`           | 18477               | [ ]    | [ ]          |
| `/api/validate-manifest`       | 18715               | [ ]    | [ ]          |
| `/api/version`                 | 18465               | [ ]    | [ ]          |
| `/api/warmup-resolve`          | 19352               | [ ]    | [ ]          |
| `/configure`                   | 14682, 14700        | [ ]    | [ ]          |
| `/eclipse`                     | 14112               | [ ]    | [ ]          |
| `/eclipse/*`                   | 14112               | [ ]    | [ ]          |
| `/manifest.json`               | 14683               | [ ]    | [ ]          |
| `/nuvio`                       | 14680, 14725        | [ ]    | [ ]          |
| `/nuvio/*`                     | 14680               | [ ]    | [ ]          |
| `/stremio`                     | 14681, 14688        | [ ]    | [ ]          |
| `/stremio/*`                   | 14681               | [ ]    | [ ]          |
| `addon:/`                      | 14139, 14725        | [ ]    | [ ]          |
| `addon:/configure`             | 14700               | [ ]    | [ ]          |
| `addon:/manifest.json`         | 14315, 14972        | [ ]    | [ ]          |
| `addon:/resolve`               | 14597               | [ ]    | [ ]          |
| `addon:/resolve-isrc`          | 14669               | [ ]    | [ ]          |
| `addon:/search`                | 14349               | [ ]    | [ ]          |
| `re:^/([0-9a-fA-F]{32,})(?:/   | $)`                 | 1727   | [ ]          | [ ]   |
| `re:^/(catalog                 | meta                | stream | subtitles)/` | 14684 | [ ] | [ ] |
| `re:^/[0-9a-fA-F]{32,}(/.*)?$` | 1781                | [ ]    | [ ]          |
| `re:^/api/download/([^/]+)(?:/ | $)`                 | 11114  | [ ]          | [ ]   |
| `re:^/t/([A-Za-z0-9]+)(?:/     | $)`                 | 1730   | [ ]          | [ ]   |
| `re:^/t/[A-Za-z0-9]+(/.*)?$`   | 1784                | [ ]    | [ ]          |
