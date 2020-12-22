# Instruction
## Basic Informations
As I write before, Czkawka is fast, powerful and EASY to use.

Czkawka for now contains two independent frontends - Console and Graphical interface which share the core module which contains basic and common functions used by each frontend. 

Using Rust language without unsafe code, helps to create safe, fast with small resource requirements.

The code has very good support for multithreading, so the better processor/disk the performance should increase exponentially.

## Tools
### Duplicate Finder

Duplicate Finder allows you to search for files and group them according to a predefined criterion:
- **By name** - Groups files by name e.g. `/home/rafal/plik.txt` will be treat like duplicate of file `/home/romb/plik.txt`. This is the fastest method, but it is very unreliable and should not be used unless you know what you are doing.
- **By size** - Groups files by its size(in bytes), which must be exactly the same. It is as fast as the previous mode and usually gives much more correct results with duplicates, but I also do not recommend using it if you do not know what you are doing.
- **By hash** - A mode containing a check of the hash (cryptographic hash) of a given file which determines with great probability whether the files are identical.
  
  This is the slowest, but almost 100% sure way to check the files.
  
  Because the hash is only checked inside groups of files of the same size, it is practically impossible for two different files to be considered identical.
  
  It consists of 3 parts:
  - Grouping files of identical size - allows you to throw away files of unique size, which are already known to have no duplicates at this stage.
  - PreHash check - Each group of files of identical size is placed in a queue using all processor threads (each action in the group is independent of the others).
  In each such group a small fragment of each file (2KB) is loaded in turn and then hashed. All files whose partial hashes are unique within the group are removed from it. Using this step usually allowed me to reduce the time of searching for duplicates even by half.
  - Checking the Hash - After leaving files that have the same beginning in groups, you should now check the whole contents of the file to make sure they are identical.

- **By hashmb** - Works the same way as via hash, only in the last phase it does not check the whole file but only its first Megabyte. It is perfect for quick search of possible duplicate files.

### Empty Files
Searching for empty files is rather easy, because we only need to read file metadata and check if its length is 0.

### Empty Directories
Puste katalogi to takie katalogi, które nie zawierają żadnych innych plików, linków symbolicznych itp. chyba że są to inne puste katalogi.

At the beginning, a special entry is created for each directory containing - the parent path (only if it is not a folder directly selected by the user) and a flag to indicate whether the given directory is empty(at the beginning each one is potentially empty).

First, user-defined folders are put into the pool of folders to be checked.

Each element is checked to see if it is
- folder - this folder is added to the check queue as possible empty - `FolderEmptiness::Maybe`
- anything else - the given folder is "poisoned" with the `FolderEmptiness::No` flag, indicating that the folder is no longer empty. Then each folder directly or indirectly containing the file is also poisoned with the `FolderEmptiness::No` flag.

e.g. There is 4 checked folder which may be empty `/krowa/`, `/krowa/ucho/`, `/krowa/ucho/stos/`, `/krowa/ucho/flaga/`.

In the last one is found a file, so that means that `/krowa/ucho/flaga/` is not empty and also all parents - `/krowa/ucho/` and `/krowa/`.
`/krowa/ucho/stos/` still may be empty.

Finally, all folders with the flag `FolderEmpriness::Maybe` are considered empty

### Big Files
From each file inside the given path its size is read and then after sorting it, e.g. 50 largest files are displayed.

### Temporary Files
Searching for temporary files only involves comparing their extensions with a previously prepared list.

Currently files with this extensions are considered as temporary files -
```
["#", "thumbs.db", ".bak", "~", ".tmp", ".temp", ".ds_store", ".crdownload", ".part", ".cache", ".dmp", ".download", ".partial"]
```

### Zeroed Files
Zeroed files very often are results of e.g. incorrect file downloads.

Their search consists of 3 parts:
- Collecting a list of all files with a size greater than 0
- At start, 64 bytes of each file are checked to discard the vast majority of non-zero files without major performance losses.
- The next step is to check the rest of the file

### Invalid Symlinks
To find invalid symlinks we must to find first a symlnks.

After searching for them you should check at which element it points to and if it does not exist, add this symlinks into the list of invalid symlinks, pointing to a non-existent path.

The second mode is to detect recursive symlink. Unfortunately, this mode does not work and it display when using it, an error of a non-existent target element, but it is implemented by counting the jumps of the symlink and after exceeding a certain number (e.g. 20) it is considered that the given symlink is recursive.

### Same Music
This is a mode to find identical music files through tags.

The number of tags to choose from is limited by an external library.

First, music files with one of the extensions `[".mp3", ".flac", ".m4a"]` are collected.

Then for each music file its tags are read.

Then, for each selected tag by which we want to search for duplicates, we perform the following steps
- For each input file we read the value of the currently checked tag
- If it is empty, we ignore the file, if it has a value, we throw it into an array whose key is this value
- After checking all files, arrays containing only one element are deleted
- The remaining files are used as initial data for checking the next tag selected by the user
- After checking all tags, the results are displayed in groups

### Similar Images
It is a tool for finding similar images that differ e.g. in watermark, size etc.

The tool first collects images with specific extensions that can be checked - `["jpg", "png", "bmp", "ico", "webp", "tiff", "dds"]`.

Then a perceptual hash is created for each image.

Cryptographic hash (used for example in ciphers) for similar inputs gives completely different outputs  
11110 ==>  AAAAAB  
11111 ==>  FWNTLW  
01110 ==>  TWMQLA

Perceptual hash at similar inputs, gives similar outputs  
11110 ==>  AAAAAB  
11111 ==>  AABABB  
01110 ==>  ACAAAB

The hash data is then thrown into a special tree that allows to compare hashes using [Hamming distance](https://en.wikipedia.org/wiki/Hamming_distance).

Finally, each hash is compared with the others and if the distance between them is less than the maximum distance specified by the user, the images are considered similar and thrown from the pool of images to be searched.


## GUI GTK
**TODO**

## CLI
**TODO**



