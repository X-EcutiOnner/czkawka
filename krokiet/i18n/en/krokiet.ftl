# In Rust translations
rust_loaded_preset = Loaded preset { $preset_idx }
rust_error_moving_to_trash = Error while moving to trash: { $error }
rust_error_removing_file = Error while removing file: { $error }
rust_file_already_exists = File \"{ $file }\" already exists, and will not be overridden
rust_error_removing_file_after_copy = Error while removing file \"{ $file }\" (after copying into different partition), reason: { $reason }
rust_error_copying_file = Error while copying \"{ $input }\" to \"{ $output }\", reason: { $reason }
rust_loading_tags_cache = Loading tags cache
rust_loading_fingerprints_cache = Loading fingerprints cache
rust_saving_tags_cache = Saving tags cache
rust_saving_fingerprints_cache = Saving fingerprints cache
rust_loading_prehash_cache = Loading prehash cache
rust_saving_prehash_cache = Saving prehash cache
rust_loading_hash_cache = Loading hash cache
rust_saving_hash_cache = Saving hash cache
rust_scanning_name = Scanning name of { $entries_checked } file
rust_scanning_size_name = Scanning size and name of { $entries_checked } file
rust_scanning_size = Scanning size of { $entries_checked } file
rust_scanning_file = Scanning { $entries_checked } file
rust_scanning_folder = Scanning { $entries_checked } folder
rust_checked_tags = Checked tags of { $items_stats }
rust_checked_content = Checked content of { $items_stats } ({ $size_stats })
rust_compared_tags = Compared tags of { $items_stats }
rust_compared_content = Compared content of { $items_stats }
rust_hashed_images = Hashed of { $items_stats } image ({ $size_stats })
rust_compared_image_hashes = Compared { $items_stats } image hash
rust_hashed_videos = Hashed of { $items_stats } video
rust_checked_files = Checked { $items_stats } file ({ $size_stats })
rust_checked_files_bad_extensions = Checked { $items_stats } file
rust_analyzed_partial_hash = Analyzed partial hash of { $items_stats } files ({ $size_stats })
rust_analyzed_full_hash = Analyzed full hash of { $items_stats } files ({ $size_stats })
rust_failed_to_rename_file = Failed to rename file { $old_path } to { $new_path } with error { $error }
rust_no_included_directories = Cannot start scan when no included directories are set.
rust_all_dirs_referenced = Cannot start scan when all included directories are set as referenced folders.
rust_found_empty_folders = Found { $items_found } empty folders
rust_found_empty_files = Found { $items_found } empty files
rust_found_similar_images = Found { $items_found } similar image files
rust_found_similar_videos = Found { $items_found } similar video files
rust_no_similarity_method_selected = Cannot find similar music files without any similarity method selected.
rust_found_similar_music_files = Found { $items_found } similar music files
rust_found_invalid_symlinks = Found { $items_found } invalid symlinks
rust_found_temporary_files = Found { $items_found } temporary files
rust_no_file_type_selected = Cannot find broken files without any file type selected.
rust_found_broken_files = Found { $items_found } broken files
rust_found_bad_extensions = Found { $items_found } files with bad extensions
rust_found_duplicate_files = Found { $items_found } similar duplicate files
rust_found_big_files = Found { $items_found } big files
rust_cannot_load_preset = Cannot change and load preset { $preset_idx } - reason { $reason }, using default settings instead
rust_saved_preset = Saved preset { $preset_idx }
rust_cannot_save_preset = Cannot save preset { $preset_idx } - reason { $reason }
rust_reset_preset = Reset preset { $preset_idx }
rust_cannot_create_output_folder = Cannot create output folder { $output_folder }, reason: { $error }

rust_delete_summary = Deleted { $deleted } items, failed to remove { $failed } items, from all { $total } items
rust_rename_summary = Renamed { $renamed } items, failed to rename { $failed } items, from all { $total } items
rust_move_summary = Moved { $moved } items, failed to move { $failed } items, from all { $total } items
rust_deleting_files = Deleting { $items_stats } file ({ $size_stats })
rust_deleting_no_size_files = Deleting { $items_stats } file
rust_renaming_files = Renaming { $items_stats } file
rust_moving_files = Moving { $items_stats } file ({ $size_stats })
rust_moving_no_size_files = Moving { $items_stats } file
rust_no_files_deleted = Not selected any files/folders to delete
rust_no_files_renamed = Not selected any files/folders to rename
rust_no_files_moved = Not selected any files/folders to move


# Slint translations, but in arrays

column_selection = Selection
column_size = Size
column_file_name = File Name
column_path = Path
column_modification_date = Modification Date
column_similarity = Similarity
column_dimensions = Dimensions
column_title = Title
column_artist = Artist
column_year = Year
column_bitrate = Bitrate
column_length = Length
column_genre = Genre
column_type_of_error = Type of Error
column_symlink_name = Symlink Name
column_symlink_folder = Symlink Folder
column_destination_path = Destination Path
column_current_extension = Current Extension
column_proper_extension = Proper Extension

# Slint translations
ok_button = Ok
cancel_button = Cancel
are_you_want_to_continue = Are you want to continue?
main_window_title = Krokiet - Data Cleaner
scan_button = Scan
stop_button = Stop
select_button = Select
move_button = Move
delete_button = Delete
save_button = Save
sort_button = Sort
rename_button = Rename
motto = This program is free to use and will always be.\nSee the The MIT/GPL License for details.
unicorn = You may not look at unicorn, but unicorn always looks at you.
repository = Repository
instruction = Instruction
donation = Donation
translation = Translation
included_directories = Included Directories
excluded_directories = Excluded Directories
ref = Ref
path = Path
tool_duplicate_files = Duplicate Files
tool_empty_folders = Empty Folders
tool_big_files = Big Files
tool_empty_files = Empty Files
tool_temporary_files = Temporary Files
tool_similar_images = Similar Images
tool_similar_videos = Similar Videos
tool_music_duplicates = Music Duplicates
tool_invalid_symlinks = Invalid Symlinks
tool_broken_files = Broken Files
tool_bad_extensions = Bad Extensions
sort_by_item_name = Sort by item name
sort_by_parent_name = Sort by parent folder
sort_by_full_name = Sort by full name
sort_by_size = Sort by size
sort_by_modification_date = Sort by modification date
sort_by_selection = Sort by selection
sort_reverse = Reverse order
sort_by_checked = Sort by check status
selection_all = Select all
selection_deselect_all = Unselect all
selection_invert_selection = Invert selection
selection_the_biggest_size = Select the biggest size
selection_the_biggest_resolution = Select the biggest resolution
selection_the_smallest_size = Select the smallest size
selection_the_smallest_resolution = Select the smallest resolution
selection_newest = Select newest
selection_oldest = Select oldest
stage_current = Current Stage:
stage_all = All Stages:
subsettings = Subsettings
subsettings_images_hash_size = Hash Size
subsettings_images_resize_algorithm = Resize Algorithm
subsettings_images_ignore_same_size = Ignore images with same size
subsettings_images_max_difference = Max difference
subsettings_images_duplicates_hash_type = Hash Type
subsettings_duplicates_check_method = Check method
subsettings_duplicates_name_case_sensitive = Case Sensitive(only name modes)
subsettings_biggest_files_sub_method = Method
subsettings_biggest_files_sub_number_of_files = Number of files
subsettings_videos_max_difference = Max difference
subsettings_videos_ignore_same_size = Ignore videos with same size
subsettings_music_audio_check_type = Audio check type
subsettings_music_approximate_comparison = Approximate Tag Comparison
subsettings_music_compared_tags = Compared tags
subsettings_music_title = Title
subsettings_music_artist = Artist
subsettings_music_bitrate = Bitrate
subsettings_music_genre = Genre
subsettings_music_year = Year
subsettings_music_length = Length
subsettings_music_max_difference = Max difference
subsettings_music_minimal_fragment_duration = Minimal fragment duration
subsettings_music_compare_fingerprints_only_with_similar_titles = Compare within groups of similar titles
subsettings_broken_files_type = Type of files to check
subsettings_broken_files_audio = Audio
subsettings_broken_files_pdf = Pdf
subsettings_broken_files_archive = Archive
subsettings_broken_files_image = Image
settings_global_settings = Global Settings
settings_dark_theme = Dark theme
settings_show_only_icons = Show only icons
settings_excluded_items = Excluded item:
settings_allowed_extensions = Allowed extensions:
settings_excluded_extensions = Excluded extensions:
settings_file_size = File Size(Kilobytes)
settings_minimum_file_size = Min:
settings_maximum_file_size = Max:
settings_recursive_search = Recursive search
settings_use_cache = Use cache
settings_save_as_json = Also save cache as JSON file
settings_move_to_trash = Move deleted files to trash
settings_ignore_other_filesystems = Ignore other filesystems (only Linux)
settings_thread_number = Thread number
settings_restart_required = ---You need to restart app to apply changes in thread number---
settings_duplicate_image_preview = Image preview
settings_duplicate_hide_hard_links = Hide hard links
settings_duplicate_minimal_hash_cache_size = Minimal size of cached files - Hash (KB)
settings_duplicate_use_prehash = Use prehash
settings_duplicate_minimal_prehash_cache_size = Minimal size of cached files - Prehash (KB)
settings_duplicate_delete_outdated_entries = Delete automatically outdated entries
settings_similar_images_show_image_preview = Image preview
settings_similar_images_hide_hard_links = Hide hard links
settings_delete_outdated_entries = Delete automatically outdated entries
settings_similar_videos_hide_hard_links = Hide hard links
settings_open_config_folder = Open config folder
settings_open_cache_folder = Open cache folder
settings_language = Language
settings_current_preset = Current Preset:
settings_edit_name = Edit name
settings_choose_name_for_prefix = Choose name for prefix
settings_save = Save
settings_load = Load
settings_reset = Reset
settings_similar_videos_tool = Similar Videos tool
settings_similar_images_tool = Similar Images tool
settings_similar_music_tool = Similar Music tool
settings_general_settings = General Settings
settings_settings = Settings
popup_save_title = Saving results
popup_save_message = This will save results to 3 different files
popup_rename_title = Renaming files
popup_rename_message = This will rename extensions of selected files to more proper
popup_new_directories_title = Please add directories one per line
popup_move_title = Moving files
popup_move_message = Moving entries to folder
popup_move_copy_checkbox = Copy files instead of moving
popup_move_preserve_folder_checkbox = Preserve folder structure
delete = Delete items
delete_confirmation = Are you sure you want to delete the selected items?
stopping_scan = Stopping scan, please wait...
searching = Searching...
subsettings_videos_crop_detect = Crop detect method
subsettings_videos_skip_forward_amount = Skip duration [s]
subsettings_videos_vid_hash_duration = Video hash duration