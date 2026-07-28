# Changelog

## [1.6.0](https://github.com/kano-lab/lab-resource-manager/compare/v1.5.3...v1.6.0) (2026-07-28)


### Features

* make operations traceable through structured logs ([#121](https://github.com/kano-lab/lab-resource-manager/issues/121)) ([2ae3321](https://github.com/kano-lab/lab-resource-manager/commit/2ae332116ff9f1eef731a9293f90a0a2fae9aa24))
* record the Slack identifier of every message sent ([#127](https://github.com/kano-lab/lab-resource-manager/issues/127)) ([b5d53df](https://github.com/kano-lab/lab-resource-manager/commit/b5d53dfc1a56cb716b6a9a0c9b673a2b0ffa32c2))


### Bug Fixes

* list resources one per line in the proposal DM ([#124](https://github.com/kano-lab/lab-resource-manager/issues/124)) ([fba281c](https://github.com/kano-lab/lab-resource-manager/commit/fba281c77e13a3e853159fdf287618751be4b3a6))
* name the conflicting reservation's owner when acceptance fails ([#129](https://github.com/kano-lab/lab-resource-manager/issues/129)) ([0b332f3](https://github.com/kano-lab/lab-resource-manager/commit/0b332f387b253b4c74b972cb5db42dfacc0abca0))
* report conflicts per reservation instead of per resource ([#125](https://github.com/kano-lab/lab-resource-manager/issues/125)) ([d36a22d](https://github.com/kano-lab/lab-resource-manager/commit/d36a22db95742609383031760c96e94f5eb9f7e8))
* send one unauthorized-usage notice per reservation ([#123](https://github.com/kano-lab/lab-resource-manager/issues/123)) ([e8bd325](https://github.com/kano-lab/lab-resource-manager/commit/e8bd3255979b91473bb8662a52ea677faac22a6a))
* start a post-hoc reservation when the proposal is accepted ([#128](https://github.com/kano-lab/lab-resource-manager/issues/128)) ([a5bd0fd](https://github.com/kano-lab/lab-resource-manager/commit/a5bd0fd42c096336b91ebc9c772ee449ffbc016f))

## [1.5.3](https://github.com/kano-lab/lab-resource-manager/compare/v1.5.2...v1.5.3) (2026-07-28)


### Bug Fixes

* bound the range of reservations each caller asks for ([#120](https://github.com/kano-lab/lab-resource-manager/issues/120)) ([70023d1](https://github.com/kano-lab/lab-resource-manager/commit/70023d1b068acaf139e82ce8316ab7d7b1b1efd1))


### Reverts

* "refactor: let callers state the range of reservations they need" ([#118](https://github.com/kano-lab/lab-resource-manager/issues/118)) ([38b4258](https://github.com/kano-lab/lab-resource-manager/commit/38b42589070e8fad0f6ec885f9edcf59a9efaf6d))

## [1.5.2](https://github.com/kano-lab/lab-resource-manager/compare/v1.5.1...v1.5.2) (2026-07-28)


### Bug Fixes

* bound how far ahead future reservations are searched ([#115](https://github.com/kano-lab/lab-resource-manager/issues/115)) ([121d7bb](https://github.com/kano-lab/lab-resource-manager/commit/121d7bbbdcb431361194a5ee7b0b629dfc64f6e2))

## [1.5.1](https://github.com/kano-lab/lab-resource-manager/compare/v1.5.0...v1.5.1) (2026-07-28)


### Bug Fixes

* decide identity equality by system and user id only ([#110](https://github.com/kano-lab/lab-resource-manager/issues/110)) ([bfa2fcc](https://github.com/kano-lab/lab-resource-manager/commit/bfa2fcc9403078bf9f7da4ed709b007ea14fd8be))
* serialize acceptances of the same proposal ([#108](https://github.com/kano-lab/lab-resource-manager/issues/108)) ([6d4997e](https://github.com/kano-lab/lab-resource-manager/commit/6d4997e9de481b1c442077977b58ea78cf0238ab))

## [1.5.0](https://github.com/kano-lab/lab-resource-manager/compare/v1.4.1...v1.5.0) (2026-07-27)


### Features

* group post-hoc reservation proposals and make acceptance idempotent ([#104](https://github.com/kano-lab/lab-resource-manager/issues/104)) ([791710c](https://github.com/kano-lab/lab-resource-manager/commit/791710c147b062738cb5bb505c48bbcea3d95f15))
* show the reserver's OS user name and reservation id on the calendar ([#105](https://github.com/kano-lab/lab-resource-manager/issues/105)) ([3174136](https://github.com/kano-lab/lab-resource-manager/commit/3174136f129f41d707972f6b5dcc614e60e8372a))


### Bug Fixes

* detect conflicts against reservations that have already ended ([#106](https://github.com/kano-lab/lab-resource-manager/issues/106)) ([401cea7](https://github.com/kano-lab/lab-resource-manager/commit/401cea7e7c084b9f7a7b874276e34ea45515d2ce))

## [1.4.1](https://github.com/kano-lab/lab-resource-manager/compare/v1.4.0...v1.4.1) (2026-07-25)


### Bug Fixes

* clarify unauthorized-usage DM wording ([#97](https://github.com/kano-lab/lab-resource-manager/issues/97)) ([fbc357a](https://github.com/kano-lab/lab-resource-manager/commit/fbc357a6639c92d671c6fa6507ab9e0fdc316bf6))

## [1.4.0](https://github.com/kano-lab/lab-resource-manager/compare/v1.3.1...v1.4.0) (2026-07-25)


### Features

* expose reservations to MCP agents via an embedded HTTP/SSE server ([#98](https://github.com/kano-lab/lab-resource-manager/issues/98)) ([b8f2aa8](https://github.com/kano-lab/lab-resource-manager/commit/b8f2aa8afa6badddd630a6b98b903b0f8b4d7490))

## [1.3.1](https://github.com/kano-lab/lab-resource-manager/compare/v1.3.0...v1.3.1) (2026-07-24)


### Bug Fixes

* repair GPU observation DM delivery and spam issues ([#95](https://github.com/kano-lab/lab-resource-manager/issues/95)) ([cf52617](https://github.com/kano-lab/lab-resource-manager/commit/cf52617e9a292c432cf4a1feb6e82257243089e7))

## [1.3.0](https://github.com/kano-lab/lab-resource-manager/compare/v1.2.0...v1.3.0) (2026-07-24)


### Features

* link OS usernames to email via the /link-user modal ([#93](https://github.com/kano-lab/lab-resource-manager/issues/93)) ([7944add](https://github.com/kano-lab/lab-resource-manager/commit/7944add963bfa0ca62a7dfd700d6a242188642a4))


### Bug Fixes

* skip unauthorized-usage notification when identity is unresolvable ([#90](https://github.com/kano-lab/lab-resource-manager/issues/90)) ([a42cb41](https://github.com/kano-lab/lab-resource-manager/commit/a42cb4170cf46ab1e04a6b0cb21d50766ae7c171))

## [1.2.0](https://github.com/kano-lab/lab-resource-manager/compare/v1.1.1...v1.2.0) (2026-07-24)


### Features

* lay groundwork for real-server GPU usage observation ([#86](https://github.com/kano-lab/lab-resource-manager/issues/86)) ([4a38e62](https://github.com/kano-lab/lab-resource-manager/commit/4a38e625e756920662002e6ccf93b8b6f7cb7474))
* report all conflicting resources, not just the first ([#82](https://github.com/kano-lab/lab-resource-manager/issues/82)) ([7f2b660](https://github.com/kano-lab/lab-resource-manager/commit/7f2b660a58f95a11e95445dbd3b4b00218647418))


### Bug Fixes

* Handle unpinned toolchain/dependency drift breaking CI ([#84](https://github.com/kano-lab/lab-resource-manager/issues/84)) ([479e6b1](https://github.com/kano-lab/lab-resource-manager/commit/479e6b1aa15511d84e674ae4ca907b038481f352))
* Preserve notes when parsing Google Calendar event descriptions ([#81](https://github.com/kano-lab/lab-resource-manager/issues/81)) ([3cb1ebc](https://github.com/kano-lab/lab-resource-manager/commit/3cb1ebc3ab21aa6b0dedc17ee60b41b06b1d7ba5))
* Preserve notes written before the managed-section begin marker ([#88](https://github.com/kano-lab/lab-resource-manager/issues/88)) ([bd566e5](https://github.com/kano-lab/lab-resource-manager/commit/bd566e50e5b24404c106d4473b512f1279a2ffee))
