use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Debug;
use std::fs::File;
use std::hash::Hasher;
use std::io::prelude::*;
use std::io::{self};
#[cfg(target_family = "unix")]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::{fs, mem, thread};

use crossbeam_channel::Sender;
use fun_time::fun_time;
use humansize::{BINARY, format_size};
use log::debug;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use static_assertions::const_assert;
use xxhash_rust::xxh3::Xxh3;

use crate::common::{WorkContinueStatus, check_if_stop_received, prepare_thread_handler_common, send_info_and_wait_for_ending_all_threads};
use crate::common_cache::{get_duplicate_cache_file, load_cache_from_file_generalized_by_size, save_cache_to_file_generalized};
use crate::common_dir_traversal::{CheckingMethod, DirTraversalBuilder, DirTraversalResult, FileEntry, ToolType};
use crate::common_tool::{CommonData, CommonToolData, DeleteMethod};
use crate::common_traits::*;
use crate::progress_data::{CurrentStage, ProgressData};

pub const PREHASHING_BUFFER_SIZE: u64 = 4 * 1024;
pub const THREAD_BUFFER_SIZE: usize = 2 * 1024 * 1024;

thread_local! {
    static THREAD_BUFFER: RefCell<Vec<u8>> = RefCell::new(vec![0u8; THREAD_BUFFER_SIZE]);
}

#[derive(PartialEq, Eq, Clone, Debug, Copy, Default)]
pub enum HashType {
    #[default]
    Blake3,
    Crc32,
    Xxh3,
}

impl HashType {
    fn hasher(self) -> Box<dyn MyHasher> {
        match self {
            Self::Blake3 => Box::new(blake3::Hasher::new()),
            Self::Crc32 => Box::new(crc32fast::Hasher::new()),
            Self::Xxh3 => Box::new(Xxh3::new()),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct DuplicateEntry {
    pub path: PathBuf,
    pub modified_date: u64,
    pub size: u64,
    pub hash: String,
}
impl ResultEntry for DuplicateEntry {
    fn get_path(&self) -> &Path {
        &self.path
    }
    fn get_modified_date(&self) -> u64 {
        self.modified_date
    }
    fn get_size(&self) -> u64 {
        self.size
    }
}

impl FileEntry {
    fn into_duplicate_entry(self) -> DuplicateEntry {
        DuplicateEntry {
            size: self.size,
            path: self.path,
            modified_date: self.modified_date,
            hash: String::new(),
        }
    }
}

#[derive(Default)]
pub struct Info {
    pub number_of_groups_by_size: usize,
    pub number_of_duplicated_files_by_size: usize,
    pub number_of_groups_by_hash: usize,
    pub number_of_duplicated_files_by_hash: usize,
    pub number_of_groups_by_name: usize,
    pub number_of_duplicated_files_by_name: usize,
    pub number_of_groups_by_size_name: usize,
    pub number_of_duplicated_files_by_size_name: usize,
    pub lost_space_by_size: u64,
    pub lost_space_by_hash: u64,
}

pub struct DuplicateFinderParameters {
    pub check_method: CheckingMethod,
    pub hash_type: HashType,
    pub ignore_hard_links: bool,
    pub use_prehash_cache: bool,
    pub minimal_cache_file_size: u64,
    pub minimal_prehash_cache_file_size: u64,
    pub case_sensitive_name_comparison: bool,
}

impl DuplicateFinderParameters {
    pub fn new(
        check_method: CheckingMethod,
        hash_type: HashType,
        ignore_hard_links: bool,
        use_prehash_cache: bool,
        minimal_cache_file_size: u64,
        minimal_prehash_cache_file_size: u64,
        case_sensitive_name_comparison: bool,
    ) -> Self {
        Self {
            check_method,
            hash_type,
            ignore_hard_links,
            use_prehash_cache,
            minimal_cache_file_size,
            minimal_prehash_cache_file_size,
            case_sensitive_name_comparison,
        }
    }
}

pub struct DuplicateFinder {
    common_data: CommonToolData,
    information: Info,
    // File Size, File Entry
    files_with_identical_names: BTreeMap<String, Vec<DuplicateEntry>>,
    // File (Size, Name), File Entry
    files_with_identical_size_names: BTreeMap<(u64, String), Vec<DuplicateEntry>>,
    // File Size, File Entry
    files_with_identical_size: BTreeMap<u64, Vec<DuplicateEntry>>,
    // File Size, next grouped by file size, next grouped by hash
    files_with_identical_hashes: BTreeMap<u64, Vec<Vec<DuplicateEntry>>>,
    // File Size, File Entry
    files_with_identical_names_referenced: BTreeMap<String, (DuplicateEntry, Vec<DuplicateEntry>)>,
    // File (Size, Name), File Entry
    files_with_identical_size_names_referenced: BTreeMap<(u64, String), (DuplicateEntry, Vec<DuplicateEntry>)>,
    // File Size, File Entry
    files_with_identical_size_referenced: BTreeMap<u64, (DuplicateEntry, Vec<DuplicateEntry>)>,
    // File Size, next grouped by file size, next grouped by hash
    files_with_identical_hashes_referenced: BTreeMap<u64, Vec<(DuplicateEntry, Vec<DuplicateEntry>)>>,
    params: DuplicateFinderParameters,
}

impl DuplicateFinder {
    pub fn new(params: DuplicateFinderParameters) -> Self {
        Self {
            common_data: CommonToolData::new(ToolType::Duplicate),
            information: Info::default(),
            files_with_identical_names: Default::default(),
            files_with_identical_size: Default::default(),
            files_with_identical_size_names: Default::default(),
            files_with_identical_hashes: Default::default(),
            files_with_identical_names_referenced: Default::default(),
            files_with_identical_size_names_referenced: Default::default(),
            files_with_identical_size_referenced: Default::default(),
            files_with_identical_hashes_referenced: Default::default(),
            params,
        }
    }

    #[fun_time(message = "find_duplicates", level = "info")]
    pub fn find_duplicates(&mut self, stop_flag: &Arc<AtomicBool>, progress_sender: Option<&Sender<ProgressData>>) {
        self.prepare_items();
        self.common_data.use_reference_folders = !self.common_data.directories.reference_directories.is_empty();

        match self.get_params().check_method {
            CheckingMethod::Name => {
                self.common_data.stopped_search = self.check_files_name(stop_flag, progress_sender) == WorkContinueStatus::Stop;
                if self.common_data.stopped_search {
                    return;
                }
            }
            CheckingMethod::SizeName => {
                self.common_data.stopped_search = self.check_files_size_name(stop_flag, progress_sender) == WorkContinueStatus::Stop;
                if self.common_data.stopped_search {
                    return;
                }
            }
            CheckingMethod::Size => {
                self.common_data.stopped_search = self.check_files_size(stop_flag, progress_sender) == WorkContinueStatus::Stop;
                if self.common_data.stopped_search {
                    return;
                }
            }
            CheckingMethod::Hash => {
                self.common_data.stopped_search = self.check_files_size(stop_flag, progress_sender) == WorkContinueStatus::Stop;
                if self.common_data.stopped_search {
                    return;
                }
                self.common_data.stopped_search = self.check_files_hash(stop_flag, progress_sender) == WorkContinueStatus::Stop;
                if self.common_data.stopped_search {
                    return;
                }
            }
            _ => panic!(),
        }
        if self.delete_files(stop_flag, progress_sender) == WorkContinueStatus::Stop {
            self.common_data.stopped_search = true;
            return;
        };
        self.debug_print();
    }

    #[fun_time(message = "check_files_name", level = "debug")]
    fn check_files_name(&mut self, stop_flag: &Arc<AtomicBool>, progress_sender: Option<&Sender<ProgressData>>) -> WorkContinueStatus {
        let group_by_func = if self.get_params().case_sensitive_name_comparison {
            |fe: &FileEntry| {
                fe.path
                    .file_name()
                    .unwrap_or_else(|| panic!("Found invalid file_name \"{}\"", fe.path.to_string_lossy()))
                    .to_string_lossy()
                    .to_string()
            }
        } else {
            |fe: &FileEntry| {
                fe.path
                    .file_name()
                    .unwrap_or_else(|| panic!("Found invalid file_name \"{}\"", fe.path.to_string_lossy()))
                    .to_string_lossy()
                    .to_lowercase()
            }
        };

        let result = DirTraversalBuilder::new()
            .common_data(&self.common_data)
            .group_by(group_by_func)
            .stop_flag(stop_flag)
            .progress_sender(progress_sender)
            .checking_method(CheckingMethod::Name)
            .build()
            .run();

        match result {
            DirTraversalResult::SuccessFiles { grouped_file_entries, warnings } => {
                self.common_data.text_messages.warnings.extend(warnings);

                // Create new BTreeMap without single size entries(files have not duplicates)
                self.files_with_identical_names = grouped_file_entries
                    .into_iter()
                    .filter_map(|(name, vector)| {
                        if vector.len() > 1 {
                            Some((name, vector.into_iter().map(FileEntry::into_duplicate_entry).collect()))
                        } else {
                            None
                        }
                    })
                    .collect();

                // Reference - only use in size, because later hash will be counted differently
                if self.common_data.use_reference_folders {
                    let vec = mem::take(&mut self.files_with_identical_names)
                        .into_iter()
                        .filter_map(|(_name, vec_file_entry)| {
                            let (mut files_from_referenced_folders, normal_files): (Vec<_>, Vec<_>) = vec_file_entry
                                .into_iter()
                                .partition(|e| self.common_data.directories.is_in_referenced_directory(e.get_path()));

                            if normal_files.is_empty() {
                                None
                            } else {
                                files_from_referenced_folders.pop().map(|file| (file, normal_files))
                            }
                        })
                        .collect::<Vec<(DuplicateEntry, Vec<DuplicateEntry>)>>();
                    for (fe, vec_fe) in vec {
                        self.files_with_identical_names_referenced.insert(fe.path.to_string_lossy().to_string(), (fe, vec_fe));
                    }
                }
                self.calculate_name_stats();

                WorkContinueStatus::Continue
            }
            DirTraversalResult::Stopped => WorkContinueStatus::Stop,
        }
    }

    fn calculate_name_stats(&mut self) {
        if self.common_data.use_reference_folders {
            for (_fe, vector) in self.files_with_identical_names_referenced.values() {
                self.information.number_of_duplicated_files_by_name += vector.len();
                self.information.number_of_groups_by_name += 1;
            }
        } else {
            for vector in self.files_with_identical_names.values() {
                self.information.number_of_duplicated_files_by_name += vector.len() - 1;
                self.information.number_of_groups_by_name += 1;
            }
        }
    }

    #[fun_time(message = "check_files_size_name", level = "debug")]
    fn check_files_size_name(&mut self, stop_flag: &Arc<AtomicBool>, progress_sender: Option<&Sender<ProgressData>>) -> WorkContinueStatus {
        let group_by_func = if self.get_params().case_sensitive_name_comparison {
            |fe: &FileEntry| {
                (
                    fe.size,
                    fe.path
                        .file_name()
                        .unwrap_or_else(|| panic!("Found invalid file_name \"{}\"", fe.path.to_string_lossy()))
                        .to_string_lossy()
                        .to_string(),
                )
            }
        } else {
            |fe: &FileEntry| {
                (
                    fe.size,
                    fe.path
                        .file_name()
                        .unwrap_or_else(|| panic!("Found invalid file_name \"{}\"", fe.path.to_string_lossy()))
                        .to_string_lossy()
                        .to_lowercase(),
                )
            }
        };

        let result = DirTraversalBuilder::new()
            .common_data(&self.common_data)
            .group_by(group_by_func)
            .stop_flag(stop_flag)
            .progress_sender(progress_sender)
            .checking_method(CheckingMethod::SizeName)
            .build()
            .run();

        match result {
            DirTraversalResult::SuccessFiles { grouped_file_entries, warnings } => {
                self.common_data.text_messages.warnings.extend(warnings);

                self.files_with_identical_size_names = grouped_file_entries
                    .into_iter()
                    .filter_map(|(size_name, vector)| {
                        if vector.len() > 1 {
                            Some((size_name, vector.into_iter().map(FileEntry::into_duplicate_entry).collect()))
                        } else {
                            None
                        }
                    })
                    .collect();

                // Reference - only use in size, because later hash will be counted differently
                if self.common_data.use_reference_folders {
                    let vec = mem::take(&mut self.files_with_identical_size_names)
                        .into_iter()
                        .filter_map(|(_size, vec_file_entry)| {
                            let (mut files_from_referenced_folders, normal_files): (Vec<_>, Vec<_>) = vec_file_entry
                                .into_iter()
                                .partition(|e| self.common_data.directories.is_in_referenced_directory(e.get_path()));

                            if normal_files.is_empty() {
                                None
                            } else {
                                files_from_referenced_folders.pop().map(|file| (file, normal_files))
                            }
                        })
                        .collect::<Vec<(DuplicateEntry, Vec<DuplicateEntry>)>>();
                    for (fe, vec_fe) in vec {
                        self.files_with_identical_size_names_referenced
                            .insert((fe.size, fe.path.to_string_lossy().to_string()), (fe, vec_fe));
                    }
                }
                self.calculate_size_name_stats();

                WorkContinueStatus::Continue
            }

            DirTraversalResult::Stopped => WorkContinueStatus::Stop,
        }
    }

    fn calculate_size_name_stats(&mut self) {
        if self.common_data.use_reference_folders {
            for ((size, _name), (_fe, vector)) in &self.files_with_identical_size_names_referenced {
                self.information.number_of_duplicated_files_by_size_name += vector.len();
                self.information.number_of_groups_by_size_name += 1;
                self.information.lost_space_by_size += (vector.len() as u64) * size;
            }
        } else {
            for ((size, _name), vector) in &self.files_with_identical_size_names {
                self.information.number_of_duplicated_files_by_size_name += vector.len() - 1;
                self.information.number_of_groups_by_size_name += 1;
                self.information.lost_space_by_size += (vector.len() as u64 - 1) * size;
            }
        }
    }

    #[fun_time(message = "check_files_size", level = "debug")]
    fn check_files_size(&mut self, stop_flag: &Arc<AtomicBool>, progress_sender: Option<&Sender<ProgressData>>) -> WorkContinueStatus {
        let result = DirTraversalBuilder::new()
            .common_data(&self.common_data)
            .group_by(|fe| fe.size)
            .stop_flag(stop_flag)
            .progress_sender(progress_sender)
            .checking_method(self.get_params().check_method)
            .build()
            .run();

        match result {
            DirTraversalResult::SuccessFiles { grouped_file_entries, warnings } => {
                self.common_data.text_messages.warnings.extend(warnings);

                let grouped_file_entries: Vec<(u64, Vec<FileEntry>)> = grouped_file_entries.into_iter().collect();
                let rayon_max_len = if self.get_params().ignore_hard_links { 3 } else { 100 };
                self.files_with_identical_size = grouped_file_entries
                    .into_par_iter()
                    .with_max_len(rayon_max_len)
                    .filter_map(|(size, vec)| {
                        if vec.len() <= 1 {
                            return None;
                        }

                        let vector = if self.get_params().ignore_hard_links { filter_hard_links(&vec) } else { vec };

                        if vector.len() > 1 {
                            Some((size, vector.into_iter().map(FileEntry::into_duplicate_entry).collect()))
                        } else {
                            None
                        }
                    })
                    .collect();

                self.filter_reference_folders_by_size();
                self.calculate_size_stats();

                debug!(
                    "check_file_size - after calculating size stats/duplicates, found in {} groups, {} files with same size | referenced {} groups, {} files",
                    self.files_with_identical_size.len(),
                    self.files_with_identical_size.values().map(Vec::len).sum::<usize>(),
                    self.files_with_identical_size_referenced.len(),
                    self.files_with_identical_size_referenced.values().map(|(_fe, vec)| vec.len()).sum::<usize>()
                );

                WorkContinueStatus::Continue
            }

            DirTraversalResult::Stopped => WorkContinueStatus::Stop,
        }
    }

    fn calculate_size_stats(&mut self) {
        if self.common_data.use_reference_folders {
            for (size, (_fe, vector)) in &self.files_with_identical_size_referenced {
                self.information.number_of_duplicated_files_by_size += vector.len();
                self.information.number_of_groups_by_size += 1;
                self.information.lost_space_by_size += (vector.len() as u64) * size;
            }
        } else {
            for (size, vector) in &self.files_with_identical_size {
                self.information.number_of_duplicated_files_by_size += vector.len() - 1;
                self.information.number_of_groups_by_size += 1;
                self.information.lost_space_by_size += (vector.len() as u64 - 1) * size;
            }
        }
    }

    #[fun_time(message = "filter_reference_folders_by_size", level = "debug")]
    fn filter_reference_folders_by_size(&mut self) {
        if self.common_data.use_reference_folders && self.get_params().check_method == CheckingMethod::Size {
            let vec = mem::take(&mut self.files_with_identical_size)
                .into_iter()
                .filter_map(|(_size, vec_file_entry)| {
                    let (mut files_from_referenced_folders, normal_files): (Vec<_>, Vec<_>) = vec_file_entry
                        .into_iter()
                        .partition(|e| self.common_data.directories.is_in_referenced_directory(e.get_path()));

                    if normal_files.is_empty() {
                        None
                    } else {
                        files_from_referenced_folders.pop().map(|file| (file, normal_files))
                    }
                })
                .collect::<Vec<(DuplicateEntry, Vec<DuplicateEntry>)>>();
            for (fe, vec_fe) in vec {
                self.files_with_identical_size_referenced.insert(fe.size, (fe, vec_fe));
            }
        }
    }

    #[fun_time(message = "prehash_load_cache_at_start", level = "debug")]
    fn prehash_load_cache_at_start(&mut self) -> (BTreeMap<u64, Vec<DuplicateEntry>>, BTreeMap<u64, Vec<DuplicateEntry>>, BTreeMap<u64, Vec<DuplicateEntry>>) {
        // Cache algorithm
        // - Load data from cache
        // - Convert from BT<u64,Vec<DuplicateEntry>> to BT<String,DuplicateEntry>
        // - Save to proper values
        let loaded_hash_map;
        let mut records_already_cached: BTreeMap<u64, Vec<DuplicateEntry>> = Default::default();
        let mut non_cached_files_to_check: BTreeMap<u64, Vec<DuplicateEntry>> = Default::default();

        if self.get_params().use_prehash_cache {
            let (messages, loaded_items) = load_cache_from_file_generalized_by_size::<DuplicateEntry>(
                &get_duplicate_cache_file(&self.get_params().hash_type, true),
                self.get_delete_outdated_cache(),
                &self.files_with_identical_size,
            );
            self.get_text_messages_mut().extend_with_another_messages(messages);
            loaded_hash_map = loaded_items.unwrap_or_default();

            Self::diff_loaded_and_prechecked_files(
                "prehash_load_cache_at_start",
                mem::take(&mut self.files_with_identical_size),
                &loaded_hash_map,
                &mut records_already_cached,
                &mut non_cached_files_to_check,
            );
        } else {
            loaded_hash_map = Default::default();
            mem::swap(&mut self.files_with_identical_size, &mut non_cached_files_to_check);
        }
        (loaded_hash_map, records_already_cached, non_cached_files_to_check)
    }

    #[fun_time(message = "prehash_save_cache_at_exit", level = "debug")]
    fn prehash_save_cache_at_exit(
        &mut self,
        loaded_hash_map: BTreeMap<u64, Vec<DuplicateEntry>>,
        pre_hash_results: Vec<(u64, BTreeMap<String, Vec<DuplicateEntry>>, Vec<String>)>,
    ) {
        if self.get_params().use_prehash_cache {
            // All results = records already cached + computed results
            let mut save_cache_to_hashmap: BTreeMap<String, DuplicateEntry> = Default::default();

            for (size, vec_file_entry) in loaded_hash_map {
                if size >= self.get_params().minimal_prehash_cache_file_size {
                    for file_entry in vec_file_entry {
                        save_cache_to_hashmap.insert(file_entry.path.to_string_lossy().to_string(), file_entry.clone());
                    }
                }
            }

            for (size, hash_map, _errors) in pre_hash_results {
                if size >= self.get_params().minimal_prehash_cache_file_size {
                    for vec_file_entry in hash_map.into_values() {
                        for file_entry in vec_file_entry {
                            save_cache_to_hashmap.insert(file_entry.path.to_string_lossy().to_string(), file_entry.clone());
                        }
                    }
                }
            }

            let messages = save_cache_to_file_generalized(
                &get_duplicate_cache_file(&self.get_params().hash_type, true),
                &save_cache_to_hashmap,
                self.common_data.save_also_as_json,
                self.get_params().minimal_prehash_cache_file_size,
            );
            self.get_text_messages_mut().extend_with_another_messages(messages);
        }
    }

    #[fun_time(message = "prehashing", level = "debug")]
    fn prehashing(
        &mut self,
        stop_flag: &Arc<AtomicBool>,
        progress_sender: Option<&Sender<ProgressData>>,
        pre_checked_map: &mut BTreeMap<u64, Vec<DuplicateEntry>>,
    ) -> WorkContinueStatus {
        if self.files_with_identical_size.is_empty() {
            return WorkContinueStatus::Continue;
        }

        let check_type = self.get_params().hash_type;
        let (progress_thread_handle, progress_thread_run, _items_counter, _check_was_stopped, _size_counter) =
            prepare_thread_handler_common(progress_sender, CurrentStage::DuplicatePreHashCacheLoading, 0, self.get_test_type(), 0);

        let (loaded_hash_map, records_already_cached, non_cached_files_to_check) = self.prehash_load_cache_at_start();

        send_info_and_wait_for_ending_all_threads(&progress_thread_run, progress_thread_handle);
        if check_if_stop_received(stop_flag) {
            return WorkContinueStatus::Stop;
        }
        let (progress_thread_handle, progress_thread_run, items_counter, check_was_stopped, size_counter) = prepare_thread_handler_common(
            progress_sender,
            CurrentStage::DuplicatePreHashing,
            non_cached_files_to_check.values().map(Vec::len).sum(),
            self.get_test_type(),
            non_cached_files_to_check
                .iter()
                .map(|(size, items)| items.len() as u64 * PREHASHING_BUFFER_SIZE.min(*size))
                .sum::<u64>(),
        );

        // Convert to vector to be able to use with_max_len method from rayon
        let non_cached_files_to_check: Vec<(u64, Vec<DuplicateEntry>)> = non_cached_files_to_check.into_iter().collect();

        debug!("Starting calculating prehash");
        #[allow(clippy::type_complexity)]
        let pre_hash_results: Vec<(u64, BTreeMap<String, Vec<DuplicateEntry>>, Vec<String>)> = non_cached_files_to_check
            .into_par_iter()
            .with_max_len(3) // Vectors and BTreeMaps for really big inputs, leave some jobs to 0 thread, to avoid that I minimized max tasks for each thread to 3, which improved performance
            .map(|(size, vec_file_entry)| {
                let mut hashmap_with_hash: BTreeMap<String, Vec<DuplicateEntry>> = Default::default();
                let mut errors: Vec<String> = Vec::new();

                THREAD_BUFFER.with_borrow_mut(|buffer| {
                    for mut file_entry in vec_file_entry {
                        if check_if_stop_received(stop_flag) {
                            check_was_stopped.store(true, Ordering::Relaxed);
                            return None;
                        }
                        match hash_calculation_limit(buffer, &file_entry, check_type, PREHASHING_BUFFER_SIZE, &size_counter) {
                            Ok(hash_string) => {
                                file_entry.hash = hash_string.clone();
                                hashmap_with_hash.entry(hash_string).or_default().push(file_entry);
                            }
                            Err(s) => errors.push(s),
                        }
                        items_counter.fetch_add(1, Ordering::Relaxed);
                    }

                    Some(())
                })?;

                Some((size, hashmap_with_hash, errors))
            })
            .while_some()
            .collect();

        debug!("Completed calculating prehash");

        send_info_and_wait_for_ending_all_threads(&progress_thread_run, progress_thread_handle);

        // Saving into cache
        let (progress_thread_handle, progress_thread_run, _items_counter, _check_was_stopped, _size_counter) =
            prepare_thread_handler_common(progress_sender, CurrentStage::DuplicatePreHashCacheSaving, 0, self.get_test_type(), 0);

        // Add data from cache
        for (size, mut vec_file_entry) in records_already_cached {
            pre_checked_map.entry(size).or_default().append(&mut vec_file_entry);
        }

        // Check results
        for (size, hash_map, errors) in &pre_hash_results {
            if !errors.is_empty() {
                self.common_data.text_messages.warnings.append(&mut errors.clone());
            }
            for vec_file_entry in hash_map.values() {
                if vec_file_entry.len() > 1 {
                    pre_checked_map.entry(*size).or_default().append(&mut vec_file_entry.clone());
                }
            }
        }

        self.prehash_save_cache_at_exit(loaded_hash_map, pre_hash_results);

        send_info_and_wait_for_ending_all_threads(&progress_thread_run, progress_thread_handle);
        if check_was_stopped.load(Ordering::Relaxed) || check_if_stop_received(stop_flag) {
            return WorkContinueStatus::Stop;
        }

        WorkContinueStatus::Continue
    }

    fn diff_loaded_and_prechecked_files(
        function_name: &str,
        used_map: BTreeMap<u64, Vec<DuplicateEntry>>,
        loaded_hash_map: &BTreeMap<u64, Vec<DuplicateEntry>>,
        records_already_cached: &mut BTreeMap<u64, Vec<DuplicateEntry>>,
        non_cached_files_to_check: &mut BTreeMap<u64, Vec<DuplicateEntry>>,
    ) {
        debug!("{function_name} - started diff between loaded and prechecked files");

        for (size, mut vec_file_entry) in used_map {
            if let Some(cached_vec_file_entry) = loaded_hash_map.get(&size) {
                // TODO maybe hashmap is not needed when using < 4 elements
                let mut cached_path_entries: HashMap<&Path, DuplicateEntry> = HashMap::new();
                for file_entry in cached_vec_file_entry {
                    cached_path_entries.insert(&file_entry.path, file_entry.clone());
                }
                for file_entry in vec_file_entry {
                    if let Some(cached_file_entry) = cached_path_entries.remove(file_entry.path.as_path()) {
                        records_already_cached.entry(size).or_default().push(cached_file_entry);
                    } else {
                        non_cached_files_to_check.entry(size).or_default().push(file_entry);
                    }
                }
            } else {
                non_cached_files_to_check.entry(size).or_default().append(&mut vec_file_entry);
            }
        }
        debug!(
            "{function_name} - completed diff between loaded and prechecked files - {}({}) non cached, {}({}) already cached",
            non_cached_files_to_check.len(),
            format_size(non_cached_files_to_check.values().map(|v| v.iter().map(|e| e.size).sum::<u64>()).sum::<u64>(), BINARY),
            records_already_cached.len(),
            format_size(records_already_cached.values().map(|v| v.iter().map(|e| e.size).sum::<u64>()).sum::<u64>(), BINARY),
        );
    }

    #[fun_time(message = "full_hashing_load_cache_at_start", level = "debug")]
    fn full_hashing_load_cache_at_start(
        &mut self,
        mut pre_checked_map: BTreeMap<u64, Vec<DuplicateEntry>>,
    ) -> (BTreeMap<u64, Vec<DuplicateEntry>>, BTreeMap<u64, Vec<DuplicateEntry>>, BTreeMap<u64, Vec<DuplicateEntry>>) {
        let loaded_hash_map;
        let mut records_already_cached: BTreeMap<u64, Vec<DuplicateEntry>> = Default::default();
        let mut non_cached_files_to_check: BTreeMap<u64, Vec<DuplicateEntry>> = Default::default();

        if self.common_data.use_cache {
            debug!("full_hashing_load_cache_at_start - using cache");
            let (messages, loaded_items) = load_cache_from_file_generalized_by_size::<DuplicateEntry>(
                &get_duplicate_cache_file(&self.get_params().hash_type, false),
                self.get_delete_outdated_cache(),
                &pre_checked_map,
            );
            self.get_text_messages_mut().extend_with_another_messages(messages);
            loaded_hash_map = loaded_items.unwrap_or_default();

            Self::diff_loaded_and_prechecked_files(
                "full_hashing_load_cache_at_start",
                pre_checked_map,
                &loaded_hash_map,
                &mut records_already_cached,
                &mut non_cached_files_to_check,
            );
        } else {
            debug!("full_hashing_load_cache_at_start - not using cache");
            loaded_hash_map = Default::default();
            mem::swap(&mut pre_checked_map, &mut non_cached_files_to_check);
        }
        (loaded_hash_map, records_already_cached, non_cached_files_to_check)
    }

    #[fun_time(message = "full_hashing_save_cache_at_exit", level = "debug")]
    fn full_hashing_save_cache_at_exit(
        &mut self,
        records_already_cached: BTreeMap<u64, Vec<DuplicateEntry>>,
        full_hash_results: &mut Vec<(u64, BTreeMap<String, Vec<DuplicateEntry>>, Vec<String>)>,
        loaded_hash_map: BTreeMap<u64, Vec<DuplicateEntry>>,
    ) {
        if !self.common_data.use_cache {
            return;
        }
        'main: for (size, vec_file_entry) in records_already_cached {
            // Check if size already exists, if exists we must to change it outside because cannot have mut and non mut reference to full_hash_results
            for (full_size, full_hashmap, _errors) in &mut (*full_hash_results) {
                if size == *full_size {
                    for file_entry in vec_file_entry {
                        full_hashmap.entry(file_entry.hash.clone()).or_default().push(file_entry);
                    }
                    continue 'main;
                }
            }
            // Size doesn't exists add results to files
            let mut temp_hashmap: BTreeMap<String, Vec<DuplicateEntry>> = Default::default();
            for file_entry in vec_file_entry {
                temp_hashmap.entry(file_entry.hash.clone()).or_default().push(file_entry);
            }
            full_hash_results.push((size, temp_hashmap, Vec::new()));
        }

        // Must save all results to file, old loaded from file with all currently counted results
        let mut all_results: BTreeMap<String, DuplicateEntry> = Default::default();
        for (_size, vec_file_entry) in loaded_hash_map {
            for file_entry in vec_file_entry {
                all_results.insert(file_entry.path.to_string_lossy().to_string(), file_entry);
            }
        }
        for (_size, hashmap, _errors) in full_hash_results {
            for vec_file_entry in hashmap.values() {
                for file_entry in vec_file_entry {
                    all_results.insert(file_entry.path.to_string_lossy().to_string(), file_entry.clone());
                }
            }
        }

        let messages = save_cache_to_file_generalized(
            &get_duplicate_cache_file(&self.get_params().hash_type, false),
            &all_results,
            self.common_data.save_also_as_json,
            self.get_params().minimal_cache_file_size,
        );
        self.get_text_messages_mut().extend_with_another_messages(messages);
    }

    #[fun_time(message = "full_hashing", level = "debug")]
    fn full_hashing(
        &mut self,
        stop_flag: &Arc<AtomicBool>,
        progress_sender: Option<&Sender<ProgressData>>,
        pre_checked_map: BTreeMap<u64, Vec<DuplicateEntry>>,
    ) -> WorkContinueStatus {
        if pre_checked_map.is_empty() {
            return WorkContinueStatus::Continue;
        }

        let (progress_thread_handle, progress_thread_run, _items_counter, _check_was_stopped, _size_counter) =
            prepare_thread_handler_common(progress_sender, CurrentStage::DuplicateCacheLoading, 0, self.get_test_type(), 0);

        let (loaded_hash_map, records_already_cached, non_cached_files_to_check) = self.full_hashing_load_cache_at_start(pre_checked_map);

        send_info_and_wait_for_ending_all_threads(&progress_thread_run, progress_thread_handle);
        if check_if_stop_received(stop_flag) {
            return WorkContinueStatus::Stop;
        }

        let (progress_thread_handle, progress_thread_run, items_counter, check_was_stopped, size_counter) = prepare_thread_handler_common(
            progress_sender,
            CurrentStage::DuplicateFullHashing,
            non_cached_files_to_check.values().map(Vec::len).sum(),
            self.get_test_type(),
            non_cached_files_to_check.iter().map(|(size, items)| (*size) * items.len() as u64).sum::<u64>(),
        );

        let non_cached_files_to_check: Vec<(u64, Vec<DuplicateEntry>)> = non_cached_files_to_check.into_iter().collect();

        let check_type = self.get_params().hash_type;
        debug!(
            "Starting full hashing of {} files",
            non_cached_files_to_check.iter().map(|(_size, v)| v.len() as u64).sum::<u64>()
        );
        let mut full_hash_results: Vec<(u64, BTreeMap<String, Vec<DuplicateEntry>>, Vec<String>)> = non_cached_files_to_check
            .into_par_iter()
            .with_max_len(3)
            .map(|(size, vec_file_entry)| {
                let mut hashmap_with_hash: BTreeMap<String, Vec<DuplicateEntry>> = Default::default();
                let mut errors: Vec<String> = Vec::new();

                THREAD_BUFFER.with_borrow_mut(|buffer| {
                    for mut file_entry in vec_file_entry {
                        if check_if_stop_received(stop_flag) {
                            check_was_stopped.store(true, Ordering::Relaxed);
                            return None;
                        }

                        match hash_calculation(buffer, &file_entry, check_type, &size_counter, stop_flag) {
                            Ok(hash_string) => {
                                if let Some(hash_string) = hash_string {
                                    file_entry.hash = hash_string.clone();
                                    hashmap_with_hash.entry(hash_string).or_default().push(file_entry);
                                } else {
                                    return None;
                                }
                            }
                            Err(s) => errors.push(s),
                        };

                        items_counter.fetch_add(1, Ordering::Relaxed);
                    }
                    Some(())
                })?;

                Some((size, hashmap_with_hash, errors))
            })
            .while_some()
            .collect();
        debug!("Finished full hashing");

        // Even if clicked stop, save items to cache and show results
        send_info_and_wait_for_ending_all_threads(&progress_thread_run, progress_thread_handle);
        let (progress_thread_handle, progress_thread_run, _items_counter, _check_was_stopped, _size_counter) =
            prepare_thread_handler_common(progress_sender, CurrentStage::DuplicateCacheSaving, 0, self.get_test_type(), 0);

        self.full_hashing_save_cache_at_exit(records_already_cached, &mut full_hash_results, loaded_hash_map);

        send_info_and_wait_for_ending_all_threads(&progress_thread_run, progress_thread_handle);

        for (size, hash_map, mut errors) in full_hash_results {
            self.common_data.text_messages.warnings.append(&mut errors);
            for (_hash, vec_file_entry) in hash_map {
                if vec_file_entry.len() > 1 {
                    self.files_with_identical_hashes.entry(size).or_default().push(vec_file_entry);
                }
            }
        }

        WorkContinueStatus::Continue
    }

    #[fun_time(message = "hash_reference_folders", level = "debug")]
    fn hash_reference_folders(&mut self) {
        // Reference - only use in size, because later hash will be counted differently
        if self.common_data.use_reference_folders {
            let vec = mem::take(&mut self.files_with_identical_hashes)
                .into_iter()
                .filter_map(|(_size, vec_vec_file_entry)| {
                    let mut all_results_with_same_size = Vec::new();
                    for vec_file_entry in vec_vec_file_entry {
                        let (mut files_from_referenced_folders, normal_files): (Vec<_>, Vec<_>) = vec_file_entry
                            .into_iter()
                            .partition(|e| self.common_data.directories.is_in_referenced_directory(e.get_path()));

                        if normal_files.is_empty() {
                            continue;
                        }
                        if let Some(file) = files_from_referenced_folders.pop() {
                            all_results_with_same_size.push((file, normal_files));
                        }
                    }
                    if all_results_with_same_size.is_empty() {
                        None
                    } else {
                        Some(all_results_with_same_size)
                    }
                })
                .collect::<Vec<Vec<(DuplicateEntry, Vec<DuplicateEntry>)>>>();
            for vec_of_vec in vec {
                self.files_with_identical_hashes_referenced.insert(vec_of_vec[0].0.size, vec_of_vec);
            }
        }

        if self.common_data.use_reference_folders {
            for (size, vector_vectors) in &self.files_with_identical_hashes_referenced {
                for (_fe, vector) in vector_vectors {
                    self.information.number_of_duplicated_files_by_hash += vector.len();
                    self.information.number_of_groups_by_hash += 1;
                    self.information.lost_space_by_hash += (vector.len() as u64) * size;
                }
            }
        } else {
            for (size, vector_vectors) in &self.files_with_identical_hashes {
                for vector in vector_vectors {
                    self.information.number_of_duplicated_files_by_hash += vector.len() - 1;
                    self.information.number_of_groups_by_hash += 1;
                    self.information.lost_space_by_hash += (vector.len() as u64 - 1) * size;
                }
            }
        }
    }

    #[fun_time(message = "check_files_hash", level = "debug")]
    fn check_files_hash(&mut self, stop_flag: &Arc<AtomicBool>, progress_sender: Option<&Sender<ProgressData>>) -> WorkContinueStatus {
        assert_eq!(self.get_params().check_method, CheckingMethod::Hash);

        let mut pre_checked_map: BTreeMap<u64, Vec<DuplicateEntry>> = Default::default();
        if self.prehashing(stop_flag, progress_sender, &mut pre_checked_map) == WorkContinueStatus::Stop {
            return WorkContinueStatus::Stop;
        }

        if self.full_hashing(stop_flag, progress_sender, pre_checked_map) == WorkContinueStatus::Stop {
            return WorkContinueStatus::Stop;
        }

        self.hash_reference_folders();

        // Clean unused data
        let files_with_identical_size = mem::take(&mut self.files_with_identical_size);
        thread::spawn(move || drop(files_with_identical_size));

        WorkContinueStatus::Continue
    }

    #[fun_time(message = "delete_files", level = "debug")]
    fn delete_files(&mut self, stop_flag: &Arc<AtomicBool>, progress_sender: Option<&Sender<ProgressData>>) -> WorkContinueStatus {
        if self.common_data.delete_method == DeleteMethod::None {
            return WorkContinueStatus::Continue;
        }

        let files_to_delete = match self.get_params().check_method {
            CheckingMethod::Name => self.files_with_identical_names.values().cloned().collect::<Vec<_>>(),
            CheckingMethod::SizeName => self.files_with_identical_size_names.values().cloned().collect::<Vec<_>>(),
            CheckingMethod::Hash => self.files_with_identical_hashes.values().flatten().cloned().collect::<Vec<_>>(),
            CheckingMethod::Size => self.files_with_identical_size.values().cloned().collect::<Vec<_>>(),
            _ => panic!(),
        };
        self.delete_advanced_elements_and_add_to_messages(stop_flag, progress_sender, files_to_delete)
    }
}

impl DuplicateFinder {
    pub fn get_params(&self) -> &DuplicateFinderParameters {
        &self.params
    }

    pub const fn get_files_sorted_by_names(&self) -> &BTreeMap<String, Vec<DuplicateEntry>> {
        &self.files_with_identical_names
    }

    pub const fn get_files_sorted_by_size(&self) -> &BTreeMap<u64, Vec<DuplicateEntry>> {
        &self.files_with_identical_size
    }

    pub const fn get_files_sorted_by_size_name(&self) -> &BTreeMap<(u64, String), Vec<DuplicateEntry>> {
        &self.files_with_identical_size_names
    }

    pub const fn get_files_sorted_by_hash(&self) -> &BTreeMap<u64, Vec<Vec<DuplicateEntry>>> {
        &self.files_with_identical_hashes
    }

    pub const fn get_information(&self) -> &Info {
        &self.information
    }

    pub fn set_dry_run(&mut self, dry_run: bool) {
        self.common_data.dry_run = dry_run;
    }

    pub fn get_use_reference(&self) -> bool {
        self.common_data.use_reference_folders
    }

    pub fn get_files_with_identical_hashes_referenced(&self) -> &BTreeMap<u64, Vec<(DuplicateEntry, Vec<DuplicateEntry>)>> {
        &self.files_with_identical_hashes_referenced
    }

    pub fn get_files_with_identical_name_referenced(&self) -> &BTreeMap<String, (DuplicateEntry, Vec<DuplicateEntry>)> {
        &self.files_with_identical_names_referenced
    }

    pub fn get_files_with_identical_size_referenced(&self) -> &BTreeMap<u64, (DuplicateEntry, Vec<DuplicateEntry>)> {
        &self.files_with_identical_size_referenced
    }

    pub fn get_files_with_identical_size_names_referenced(&self) -> &BTreeMap<(u64, String), (DuplicateEntry, Vec<DuplicateEntry>)> {
        &self.files_with_identical_size_names_referenced
    }
}

impl DebugPrint for DuplicateFinder {
    #[allow(clippy::print_stdout)]
    fn debug_print(&self) {
        if !cfg!(debug_assertions) {
            return;
        }
        println!("---------------DEBUG PRINT---------------");
        println!(
            "Number of duplicated files by size(in groups) - {} ({})",
            self.information.number_of_duplicated_files_by_size, self.information.number_of_groups_by_size
        );
        println!(
            "Number of duplicated files by hash(in groups) - {} ({})",
            self.information.number_of_duplicated_files_by_hash, self.information.number_of_groups_by_hash
        );
        println!(
            "Number of duplicated files by name(in groups) - {} ({})",
            self.information.number_of_duplicated_files_by_name, self.information.number_of_groups_by_name
        );
        println!(
            "Lost space by size - {} ({} bytes)",
            format_size(self.information.lost_space_by_size, BINARY),
            self.information.lost_space_by_size
        );
        println!(
            "Lost space by hash - {} ({} bytes)",
            format_size(self.information.lost_space_by_hash, BINARY),
            self.information.lost_space_by_hash
        );

        println!("### Other");

        println!("Files list size - {}", self.files_with_identical_size.len());
        println!("Hashed Files list size - {}", self.files_with_identical_hashes.len());
        println!("Files with identical names - {}", self.files_with_identical_names.len());
        println!("Files with identical size names - {}", self.files_with_identical_size_names.len());
        println!("Files with identical names referenced - {}", self.files_with_identical_names_referenced.len());
        println!("Files with identical size names referenced - {}", self.files_with_identical_size_names_referenced.len());
        println!("Files with identical size referenced - {}", self.files_with_identical_size_referenced.len());
        println!("Files with identical hashes referenced - {}", self.files_with_identical_hashes_referenced.len());
        println!("Checking Method - {:?}", self.get_params().check_method);
        self.debug_print_common();
        println!("-----------------------------------------");
    }
}

impl PrintResults for DuplicateFinder {
    fn write_results<T: Write>(&self, writer: &mut T) -> io::Result<()> {
        writeln!(
            writer,
            "Results of searching {:?} (reference directories {:?}) with excluded directories {:?} and excluded items {:?}",
            self.common_data.directories.included_directories,
            self.common_data.directories.reference_directories,
            self.common_data.directories.excluded_directories,
            self.common_data.excluded_items.get_excluded_items()
        )?;

        match self.get_params().check_method {
            CheckingMethod::Name => {
                if !self.files_with_identical_names.is_empty() {
                    writeln!(
                        writer,
                        "-------------------------------------------------Files with same names-------------------------------------------------"
                    )?;
                    writeln!(
                        writer,
                        "Found {} files in {} groups with same name(may have different content)",
                        self.information.number_of_duplicated_files_by_name, self.information.number_of_groups_by_name,
                    )?;
                    for (name, vector) in self.files_with_identical_names.iter().rev() {
                        writeln!(writer, "Name - {} - {} files ", name, vector.len())?;
                        for j in vector {
                            writeln!(writer, "\"{}\"", j.path.to_string_lossy())?;
                        }
                        writeln!(writer)?;
                    }
                } else if !self.files_with_identical_names_referenced.is_empty() {
                    writeln!(
                        writer,
                        "-------------------------------------------------Files with same names in referenced folders-------------------------------------------------"
                    )?;
                    writeln!(
                        writer,
                        "Found {} files in {} groups with same name(may have different content)",
                        self.information.number_of_duplicated_files_by_name, self.information.number_of_groups_by_name,
                    )?;
                    for (name, (file_entry, vector)) in self.files_with_identical_names_referenced.iter().rev() {
                        writeln!(writer, "Name - {} - {} files ", name, vector.len())?;
                        writeln!(writer, "Reference file - {:?}", file_entry.path)?;
                        for j in vector {
                            writeln!(writer, "\"{}\"", j.path.to_string_lossy())?;
                        }
                        writeln!(writer)?;
                    }
                } else {
                    write!(writer, "Not found any files with same names.")?;
                }
            }
            CheckingMethod::SizeName => {
                if !self.files_with_identical_names.is_empty() {
                    writeln!(
                        writer,
                        "-------------------------------------------------Files with same size and names-------------------------------------------------"
                    )?;
                    writeln!(
                        writer,
                        "Found {} files in {} groups with same size and name(may have different content)",
                        self.information.number_of_duplicated_files_by_size_name, self.information.number_of_groups_by_size_name,
                    )?;
                    for ((size, name), vector) in self.files_with_identical_size_names.iter().rev() {
                        writeln!(writer, "Name - {}, {} - {} files ", name, format_size(*size, BINARY), vector.len())?;
                        for j in vector {
                            writeln!(writer, "\"{}\"", j.path.to_string_lossy())?;
                        }
                        writeln!(writer)?;
                    }
                } else if !self.files_with_identical_names_referenced.is_empty() {
                    writeln!(
                        writer,
                        "-------------------------------------------------Files with same size and names in referenced folders-------------------------------------------------"
                    )?;
                    writeln!(
                        writer,
                        "Found {} files in {} groups with same size and name(may have different content)",
                        self.information.number_of_duplicated_files_by_size_name, self.information.number_of_groups_by_size_name,
                    )?;
                    for ((size, name), (file_entry, vector)) in self.files_with_identical_size_names_referenced.iter().rev() {
                        writeln!(writer, "Name - {}, {} - {} files ", name, format_size(*size, BINARY), vector.len())?;
                        writeln!(writer, "Reference file - {:?}", file_entry.path)?;
                        for j in vector {
                            writeln!(writer, "\"{}\"", j.path.to_string_lossy())?;
                        }
                        writeln!(writer)?;
                    }
                } else {
                    write!(writer, "Not found any files with same size and names.")?;
                }
            }
            CheckingMethod::Size => {
                if !self.files_with_identical_size.is_empty() {
                    writeln!(
                        writer,
                        "-------------------------------------------------Files with same size-------------------------------------------------"
                    )?;
                    writeln!(
                        writer,
                        "Found {} duplicated files which in {} groups which takes {}.",
                        self.information.number_of_duplicated_files_by_size,
                        self.information.number_of_groups_by_size,
                        format_size(self.information.lost_space_by_size, BINARY)
                    )?;
                    for (size, vector) in self.files_with_identical_size.iter().rev() {
                        write!(writer, "\n---- Size {} ({}) - {} files \n", format_size(*size, BINARY), size, vector.len())?;
                        for file_entry in vector {
                            writeln!(writer, "\"{}\"", file_entry.path.to_string_lossy())?;
                        }
                    }
                } else if !self.files_with_identical_size_referenced.is_empty() {
                    writeln!(
                        writer,
                        "-------------------------------------------------Files with same size in referenced folders-------------------------------------------------"
                    )?;
                    writeln!(
                        writer,
                        "Found {} duplicated files which in {} groups which takes {}.",
                        self.information.number_of_duplicated_files_by_size,
                        self.information.number_of_groups_by_size,
                        format_size(self.information.lost_space_by_size, BINARY)
                    )?;
                    for (size, (file_entry, vector)) in self.files_with_identical_size_referenced.iter().rev() {
                        writeln!(writer, "\n---- Size {} ({}) - {} files", format_size(*size, BINARY), size, vector.len())?;
                        writeln!(writer, "Reference file - {:?}", file_entry.path)?;
                        for file_entry in vector {
                            writeln!(writer, "\"{}\"", file_entry.path.to_string_lossy())?;
                        }
                    }
                } else {
                    write!(writer, "Not found any duplicates.")?;
                }
            }
            CheckingMethod::Hash => {
                if !self.files_with_identical_hashes.is_empty() {
                    writeln!(
                        writer,
                        "-------------------------------------------------Files with same hashes-------------------------------------------------"
                    )?;
                    writeln!(
                        writer,
                        "Found {} duplicated files which in {} groups which takes {}.",
                        self.information.number_of_duplicated_files_by_hash,
                        self.information.number_of_groups_by_hash,
                        format_size(self.information.lost_space_by_hash, BINARY)
                    )?;
                    for (size, vectors_vector) in self.files_with_identical_hashes.iter().rev() {
                        for vector in vectors_vector {
                            writeln!(writer, "\n---- Size {} ({}) - {} files", format_size(*size, BINARY), size, vector.len())?;
                            for file_entry in vector {
                                writeln!(writer, "\"{}\"", file_entry.path.to_string_lossy())?;
                            }
                        }
                    }
                } else if !self.files_with_identical_hashes_referenced.is_empty() {
                    writeln!(
                        writer,
                        "-------------------------------------------------Files with same hashes in referenced folders-------------------------------------------------"
                    )?;
                    writeln!(
                        writer,
                        "Found {} duplicated files which in {} groups which takes {}.",
                        self.information.number_of_duplicated_files_by_hash,
                        self.information.number_of_groups_by_hash,
                        format_size(self.information.lost_space_by_hash, BINARY)
                    )?;
                    for (size, vectors_vector) in self.files_with_identical_hashes_referenced.iter().rev() {
                        for (file_entry, vector) in vectors_vector {
                            writeln!(writer, "\n---- Size {} ({}) - {} files", format_size(*size, BINARY), size, vector.len())?;
                            writeln!(writer, "Reference file - \"{}\"", file_entry.path.to_string_lossy())?;
                            for file_entry in vector {
                                writeln!(writer, "\"{}\"", file_entry.path.to_string_lossy())?;
                            }
                        }
                    }
                } else {
                    write!(writer, "Not found any duplicates.")?;
                }
            }
            _ => panic!(),
        }

        Ok(())
    }

    // TODO - check if is possible to save also data in header about size and name in SizeName mode - https://github.com/qarmin/czkawka/issues/1137
    fn save_results_to_file_as_json(&self, file_name: &str, pretty_print: bool) -> io::Result<()> {
        if self.get_use_reference() {
            match self.get_params().check_method {
                CheckingMethod::Name => self.save_results_to_file_as_json_internal(file_name, &self.files_with_identical_names_referenced, pretty_print),
                CheckingMethod::SizeName => {
                    self.save_results_to_file_as_json_internal(file_name, &self.files_with_identical_size_names_referenced.values().collect::<Vec<_>>(), pretty_print)
                }
                CheckingMethod::Size => self.save_results_to_file_as_json_internal(file_name, &self.files_with_identical_size_referenced, pretty_print),
                CheckingMethod::Hash => self.save_results_to_file_as_json_internal(file_name, &self.files_with_identical_hashes_referenced, pretty_print),
                _ => panic!(),
            }
        } else {
            match self.get_params().check_method {
                CheckingMethod::Name => self.save_results_to_file_as_json_internal(file_name, &self.files_with_identical_names, pretty_print),
                CheckingMethod::SizeName => self.save_results_to_file_as_json_internal(file_name, &self.files_with_identical_size_names.values().collect::<Vec<_>>(), pretty_print),
                CheckingMethod::Size => self.save_results_to_file_as_json_internal(file_name, &self.files_with_identical_size, pretty_print),
                CheckingMethod::Hash => self.save_results_to_file_as_json_internal(file_name, &self.files_with_identical_hashes, pretty_print),
                _ => panic!(),
            }
        }
    }
}

#[cfg(target_family = "windows")]
fn filter_hard_links(vec_file_entry: &[FileEntry]) -> Vec<FileEntry> {
    let mut inodes: HashSet<u128> = HashSet::with_capacity(vec_file_entry.len());
    let mut identical: Vec<FileEntry> = Vec::with_capacity(vec_file_entry.len());
    for f in vec_file_entry {
        if let Ok(meta) = file_id::get_high_res_file_id(&f.path) {
            if let file_id::FileId::HighRes { file_id, .. } = meta {
                if !inodes.insert(file_id) {
                    continue;
                }
            }
        }
        identical.push(f.clone());
    }
    identical
}

#[cfg(target_family = "unix")]
fn filter_hard_links(vec_file_entry: &[FileEntry]) -> Vec<FileEntry> {
    let mut inodes: HashSet<u64> = HashSet::with_capacity(vec_file_entry.len());
    let mut identical: Vec<FileEntry> = Vec::with_capacity(vec_file_entry.len());
    for f in vec_file_entry {
        if let Ok(meta) = fs::metadata(&f.path) {
            if !inodes.insert(meta.ino()) {
                continue;
            }
        }
        identical.push(f.clone());
    }
    identical
}

pub trait MyHasher {
    fn update(&mut self, bytes: &[u8]);
    fn finalize(&self) -> String;
}

pub(crate) fn hash_calculation_limit(buffer: &mut [u8], file_entry: &DuplicateEntry, hash_type: HashType, limit: u64, size_counter: &Arc<AtomicU64>) -> Result<String, String> {
    // This function is used only to calculate hash of file with limit
    // We must ensure that buffer is big enough to store all data
    // We don't need to check that each time
    const_assert!(PREHASHING_BUFFER_SIZE <= THREAD_BUFFER_SIZE as u64);

    let mut file_handler = match File::open(&file_entry.path) {
        Ok(t) => t,
        Err(e) => {
            size_counter.fetch_add(limit, Ordering::Relaxed);
            return Err(format!("Unable to check hash of file {:?}, reason {e}", file_entry.path));
        }
    };
    let hasher = &mut *hash_type.hasher();
    let n = match file_handler.read(&mut buffer[..limit as usize]) {
        Ok(t) => t,
        Err(e) => return Err(format!("Error happened when checking hash of file {:?}, reason {}", file_entry.path, e)),
    };

    hasher.update(&buffer[..n]);
    size_counter.fetch_add(n as u64, Ordering::Relaxed);
    Ok(hasher.finalize())
}

pub fn hash_calculation(
    buffer: &mut [u8],
    file_entry: &DuplicateEntry,
    hash_type: HashType,
    size_counter: &Arc<AtomicU64>,
    stop_flag: &Arc<AtomicBool>,
) -> Result<Option<String>, String> {
    let mut file_handler = match File::open(&file_entry.path) {
        Ok(t) => t,
        Err(e) => {
            size_counter.fetch_add(file_entry.size, Ordering::Relaxed);
            return Err(format!("Unable to check hash of file {:?}, reason {e}", file_entry.path));
        }
    };
    let hasher = &mut *hash_type.hasher();
    loop {
        let n = match file_handler.read(buffer) {
            Ok(0) => break,
            Ok(t) => t,
            Err(e) => return Err(format!("Error happened when checking hash of file {:?}, reason {}", file_entry.path, e)),
        };

        hasher.update(&buffer[..n]);
        size_counter.fetch_add(n as u64, Ordering::Relaxed);
        if check_if_stop_received(stop_flag) {
            return Ok(None);
        }
    }
    Ok(Some(hasher.finalize()))
}

impl MyHasher for blake3::Hasher {
    fn update(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }
    fn finalize(&self) -> String {
        self.finalize().to_hex().to_string()
    }
}

impl MyHasher for crc32fast::Hasher {
    fn update(&mut self, bytes: &[u8]) {
        self.write(bytes);
    }
    fn finalize(&self) -> String {
        self.finish().to_string()
    }
}

impl MyHasher for Xxh3 {
    fn update(&mut self, bytes: &[u8]) {
        self.write(bytes);
    }
    fn finalize(&self) -> String {
        self.finish().to_string()
    }
}

impl CommonData for DuplicateFinder {
    fn get_cd(&self) -> &CommonToolData {
        &self.common_data
    }
    fn get_cd_mut(&mut self) -> &mut CommonToolData {
        &mut self.common_data
    }
    fn get_check_method(&self) -> CheckingMethod {
        self.get_params().check_method
    }
    fn found_any_broken_files(&self) -> bool {
        self.get_information().number_of_duplicated_files_by_hash > 0
            || self.get_information().number_of_duplicated_files_by_name > 0
            || self.get_information().number_of_duplicated_files_by_size > 0
            || self.get_information().number_of_duplicated_files_by_size_name > 0
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io;

    use super::*;

    #[test]
    fn test_filter_hard_links_empty() {
        let expected: Vec<FileEntry> = Default::default();
        assert_eq!(expected, filter_hard_links(&[]));
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_filter_hard_links() -> io::Result<()> {
        let dir = tempfile::Builder::new().tempdir()?;
        let (src, dst) = (dir.path().join("a"), dir.path().join("b"));
        File::create(&src)?;
        fs::hard_link(src.clone(), dst.clone())?;
        let e1 = FileEntry { path: src, ..Default::default() };
        let e2 = FileEntry { path: dst, ..Default::default() };
        let actual = filter_hard_links(&[e1.clone(), e2]);
        assert_eq!(vec![e1], actual);
        Ok(())
    }

    #[test]
    fn test_filter_hard_links_regular_files() -> io::Result<()> {
        let dir = tempfile::Builder::new().tempdir()?;
        let (src, dst) = (dir.path().join("a"), dir.path().join("b"));
        File::create(&src)?;
        File::create(&dst)?;
        let e1 = FileEntry { path: src, ..Default::default() };
        let e2 = FileEntry { path: dst, ..Default::default() };
        let actual = filter_hard_links(&[e1.clone(), e2.clone()]);
        assert_eq!(vec![e1, e2], actual);
        Ok(())
    }

    #[test]
    fn test_hash_calculation() -> io::Result<()> {
        let dir = tempfile::Builder::new().tempdir()?;
        let mut buf = [0u8; 1 << 10];
        let src = dir.path().join("a");
        let mut file = File::create(&src)?;
        file.write_all(b"aaAAAAAAAAAAAAAAFFFFFFFFFFFFFFFFFFFFGGGGGGGGG")?;
        let e = DuplicateEntry { path: src, ..Default::default() };
        let size_counter = Arc::new(AtomicU64::new(0));
        let r = hash_calculation(&mut buf, &e, HashType::Blake3, &size_counter, &Arc::default())
            .expect("hash_calculation failed")
            .expect("hash_calculation returned None");
        assert!(!r.is_empty());
        assert_eq!(size_counter.load(Ordering::Relaxed), 45);
        Ok(())
    }

    #[test]
    fn test_hash_calculation_limit() -> io::Result<()> {
        let dir = tempfile::Builder::new().tempdir()?;
        let mut buf = [0u8; 1000];
        let src = dir.path().join("a");
        let mut file = File::create(&src)?;
        file.write_all(b"aa")?;
        let e = DuplicateEntry { path: src, ..Default::default() };
        let size_counter_1 = Arc::new(AtomicU64::new(0));
        let size_counter_2 = Arc::new(AtomicU64::new(0));
        let size_counter_3 = Arc::new(AtomicU64::new(0));
        let r1 = hash_calculation_limit(&mut buf, &e, HashType::Blake3, 1, &size_counter_1).expect("hash_calculation failed");
        let r2 = hash_calculation_limit(&mut buf, &e, HashType::Blake3, 2, &size_counter_2).expect("hash_calculation failed");
        let r3 = hash_calculation_limit(&mut buf, &e, HashType::Blake3, 1000, &size_counter_3).expect("hash_calculation failed");
        assert_ne!(r1, r2);
        assert_eq!(r2, r3);

        assert_eq!(1, size_counter_1.load(Ordering::Relaxed));
        assert_eq!(2, size_counter_2.load(Ordering::Relaxed));
        assert_eq!(2, size_counter_3.load(Ordering::Relaxed));

        Ok(())
    }

    #[test]
    fn test_hash_calculation_invalid_file() -> io::Result<()> {
        let dir = tempfile::Builder::new().tempdir()?;
        let mut buf = [0u8; 1 << 10];
        let src = dir.path().join("a");
        let e = DuplicateEntry { path: src, ..Default::default() };
        let r = hash_calculation(&mut buf, &e, HashType::Blake3, &Arc::default(), &Arc::default()).expect_err("hash_calculation succeeded");
        assert!(!r.is_empty());
        Ok(())
    }
}
