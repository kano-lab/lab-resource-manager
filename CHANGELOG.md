# Changelog

## [2.0.0](https://github.com/kano-lab/lab-resource-manager/compare/lab-resource-manager-v1.1.1...lab-resource-manager-v2.0.0) (2026-02-18)


### ⚠ BREAKING CHANGES

* Replace Docker with binary releases and systemd ([#50](https://github.com/kano-lab/lab-resource-manager/issues/50))
* Integrate watcher notifications into slackbot with Bot Token ([#43](https://github.com/kano-lab/lab-resource-manager/issues/43))

### Features

* Add application use cases, authorization service, and UUID-based repository layer ([#39](https://github.com/kano-lab/lab-resource-manager/issues/39)) ([c8b4d68](https://github.com/kano-lab/lab-resource-manager/commit/c8b4d680e6e7d012f7ed292cc3a1782469dec8ce))
* Add notification message customization ([#66](https://github.com/kano-lab/lab-resource-manager/issues/66)) ([c916e5b](https://github.com/kano-lab/lab-resource-manager/commit/c916e5b1db960b99043ee9bec3af10f4bdd2426b))
* Add resource reservation via /reserve command ([#45](https://github.com/kano-lab/lab-resource-manager/issues/45)) ([dfa862f](https://github.com/kano-lab/lab-resource-manager/commit/dfa862f2263679ce4b0c03b4015b363fa8be6349))
* add resource usage watcher with Calendar/Slack integration ([#1](https://github.com/kano-lab/lab-resource-manager/issues/1)) ([9b308d1](https://github.com/kano-lab/lab-resource-manager/commit/9b308d1cec860aa44562ec2f7108412b9f604e71))
* Add Slack Bot and user identity linking system ([#9](https://github.com/kano-lab/lab-resource-manager/issues/9)) ([2b181de](https://github.com/kano-lab/lab-resource-manager/commit/2b181debd04d42f28d6b5a442ec0165aa6a6af2f))
* Add Slack modal infrastructure ([#44](https://github.com/kano-lab/lab-resource-manager/issues/44)) ([4d7af95](https://github.com/kano-lab/lab-resource-manager/commit/4d7af95223c2d403ec0878610b6e9771c54e1cef))
* Add timezone configuration for localized notification times ([#28](https://github.com/kano-lab/lab-resource-manager/issues/28)) ([17f98be](https://github.com/kano-lab/lab-resource-manager/commit/17f98be15a98a502e02e69ca741c93df86b3716e))
* Add update and cancel buttons for reservations ([#46](https://github.com/kano-lab/lab-resource-manager/issues/46)) ([b7af7fa](https://github.com/kano-lab/lab-resource-manager/commit/b7af7fa9e0cfbc2f11a32deb46230f09e59c0359))
* **ci:** Add musl target for GLIBC compatibility ([#53](https://github.com/kano-lab/lab-resource-manager/issues/53)) ([fa29eb0](https://github.com/kano-lab/lab-resource-manager/commit/fa29eb0f8105df2a4097903d0a9bfd974fca06d2))
* implement NotificationRouter for orchestrating multiple notification methods ([#6](https://github.com/kano-lab/lab-resource-manager/issues/6)) ([9b41b99](https://github.com/kano-lab/lab-resource-manager/commit/9b41b995cff2c9eb3b07c6690934485405ce5d8b))
* Improve Slack notification format ([#30](https://github.com/kano-lab/lab-resource-manager/issues/30)) ([c80e076](https://github.com/kano-lab/lab-resource-manager/commit/c80e076ef5535f75b34692ea41a6119c15d28c71))
* initial commit ([7740258](https://github.com/kano-lab/lab-resource-manager/commit/77402586f76b0802fdf78efea06312045b94f961))
* Integrate watcher notifications into slackbot with Bot Token ([#43](https://github.com/kano-lab/lab-resource-manager/issues/43)) ([91044ef](https://github.com/kano-lab/lab-resource-manager/commit/91044efcdf8215aadd2883601b197fbe60f9a590))
* **notifier:** Display notes field in Slack notifications ([#47](https://github.com/kano-lab/lab-resource-manager/issues/47)) ([0cdb095](https://github.com/kano-lab/lab-resource-manager/commit/0cdb0954edbaa6a7282f1871fb8c1c224a13f0ef))
* Replace Docker with binary releases and systemd ([#50](https://github.com/kano-lab/lab-resource-manager/issues/50)) ([8cb2a32](https://github.com/kano-lab/lab-resource-manager/commit/8cb2a32fa72e77cf0913e487e7d5673211ecaa91))


### Bug Fixes

* Add spaces around hyphen in time format for mobile compatibility ([#75](https://github.com/kano-lab/lab-resource-manager/issues/75)) ([c751a49](https://github.com/kano-lab/lab-resource-manager/commit/c751a49c4c103cf5a1a993235606cae72543a486))
* **deps:** Use rustls-tls for musl compatibility ([#55](https://github.com/kano-lab/lab-resource-manager/issues/55)) ([48a936e](https://github.com/kano-lab/lab-resource-manager/commit/48a936ee72f54e43d93541631fdad379b0c72351))
* Display appropriate labels for resource types in Slack notifications ([#34](https://github.com/kano-lab/lab-resource-manager/issues/34)) ([470cdd4](https://github.com/kano-lab/lab-resource-manager/commit/470cdd4d4c2274b10d1562d90d37b30f4097dc33))
* initialize NotifyResourceUsageChangesUseCase with current state to prevent false notifications ([#4](https://github.com/kano-lab/lab-resource-manager/issues/4)) ([8a4677e](https://github.com/kano-lab/lab-resource-manager/commit/8a4677e1242230e5a821b4cc986c8a989df436b6))
* Prevent false deletion notifications for naturally expired events ([62913d6](https://github.com/kano-lab/lab-resource-manager/commit/62913d67fc5cea9faab4680e73ae2cbbeb810264))
