## Releasing

These are the general steps to release a new package version:

1. Remove the `SNAPSHOT` suffix from the package version number on the `Cargo.toml`.
2. Open a pull request with the changes.
3. Once the pull request is merged the release workflow will build the release version, create the tag and create a draft [Github release](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases) with the binaries and change notes.
4. Review the draft GitHub release and, if everything is ok, release it.