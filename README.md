# check-lockfile-intersection

This is a tool to help with a very specific (and possibly rare) problem. You will know if you happen to have it!

The problem is this: cargo, by design, ignores the lockfile (`Cargo.lock`) of a _library_ when using the library in a larger _application_ (or general dependent package); but this means that it's totally possible for the person who tested the library to test it at a different set of dependency versions than the application user winds up using.

However, sometimes the application _wants_ to just use _exactly_ the same versions of its dependencies as the library tested with -- semver is a nice fantasy but there are plenty of scenarios where you want precision -- and as far as I know cargo doesn't really support this. Some not-very-great options are to pin the versions in the library with `=X.Y.Z` constraints -- which will cause downstream conflicts and also not handle transitive dependencies -- or else just manually inspect the library and application lockfiles and compare them.

This tool automates, to some extent, the latter option. It does the following:

  - Load two package sets from two lockfiles, either from local file paths or http(s) URLs.
  - Exclude any packages you ask to exclude for various reasons (see below).
  - Exclude any packages that don't occur in _both_ remaining sets.
  - Starting from either a specified package/hash, or _every_ package, in each set:
    - Check that all packages in the dependency tree have the same version in both sets.
    - Print out a listing of the common and/or different package versions, for inspection.
    - Exit 0 if they're all the same, else exit 1 if they differ.

Basically all of the complexity and options have to do with _excluding_ packages from consideration, because a `Cargo.lock` file contains all the dependencies, dev-dependencies, and feature-enabled dependencies of all the packages in your workspace, and you often want to just think about a subset of those.

## Usage

`check-lockfile-intersection [options] <lockfile_a> <lockfile_b>`

```
ARGS:
    <lockfile_a>
      First lockfile (URL or path)

    <lockfile_b>
      Second lockfile (URL or path)

OPTIONS:
    --pkg-hash-a <hash_a>
      Limit first lockfile to package tree rooted at hash (git commit or crate checksum)

    --pkg-hash-b <hash_b>
      Limit second lockfile to package tree rooted at hash (git commit or crate checksum)

    --pkg-name-a <name_a>
      Limit first lockfile to package tree rooted at package name

    --pkg-name-b <name_b>
      Limit second lockfile to package tree rooted at package name

    --exclude-pkg-a <exclude_a>
      Comma-separated list of packages to exclude from first lockfile

    --exclude-pkg-b <exclude_b>
      Comma-separated list of packages to exclude from second lockfile

    --verbose
      Print more details while running

    -h, --help
      Prints help information.
```
