# Changelog

## [0.2.0](https://github.com/haswalt/kasuri/compare/v0.1.0...v0.2.0) (2026-11-26)


### ⚠ BREAKING CHANGES

* **deployment:** remove self-hosted deployment packaging ([#53](https://github.com/haswalt/kasuri/issues/53))

### Features

* **auth:** bind workspace ownership to verified users ([#75](https://github.com/haswalt/kasuri/issues/75)) ([c15a955](https://github.com/haswalt/kasuri/commit/c15a95526b85c4e0a4fbf4daab656a436e35e6ca))
* **connectors:** add approved outbound MCP tool access ([#62](https://github.com/haswalt/kasuri/issues/62)) ([f5a33be](https://github.com/haswalt/kasuri/commit/f5a33be3185b99045be7bd7c797ed7d6726ced58))
* **connectors:** add Cloudflare email trigger and sending ([#60](https://github.com/haswalt/kasuri/issues/60)) ([08094b2](https://github.com/haswalt/kasuri/commit/08094b2066047969e5cc3fab5834ac321dc7896b))
* **connectors:** add standalone Clairic text extraction ([#57](https://github.com/haswalt/kasuri/issues/57)) ([b081dae](https://github.com/haswalt/kasuri/commit/b081dae522c8f4aaf5f448c7b122a5d95d4dcbe3))
* **connectors:** report workflow errors to Sentry ([#72](https://github.com/haswalt/kasuri/issues/72)) ([9d7a49f](https://github.com/haswalt/kasuri/commit/9d7a49fe19b1b802661d42dcc4759a5285b15cce))
* **connectors:** support Cloudflare MCP Portal authentication ([#63](https://github.com/haswalt/kasuri/issues/63)) ([90b1bf8](https://github.com/haswalt/kasuri/commit/90b1bf831a55346c7fac976cb8e7bd35ec216fac))
* **deployment:** remove self-hosted deployment packaging ([#53](https://github.com/haswalt/kasuri/issues/53)) ([14dee48](https://github.com/haswalt/kasuri/commit/14dee48394355d8f2fb18acaf62f4d120e19dd75))
* **editor:** ship guided workflow and connector configuration ([#74](https://github.com/haswalt/kasuri/issues/74)) ([59a9537](https://github.com/haswalt/kasuri/commit/59a95377ae1d8fff9fab74f5b41980f17232eece))
* **editor:** simplify node input and HTTP field configuration ([#78](https://github.com/haswalt/kasuri/issues/78)) ([4b57a28](https://github.com/haswalt/kasuri/commit/4b57a2828877e20063706c4a9e1d19a6a36e02eb))
* **files:** store private email attachments for Clairic ([#61](https://github.com/haswalt/kasuri/issues/61)) ([#65](https://github.com/haswalt/kasuri/issues/65)) ([f962722](https://github.com/haswalt/kasuri/commit/f962722ebaee3addb356f0e696094263369256de))
* **mcp:** expose scoped data table management ([#79](https://github.com/haswalt/kasuri/issues/79)) ([b89fcaa](https://github.com/haswalt/kasuri/commit/b89fcaa0f1a99b849c52b91d6df59c4237076c55))
* **secrets:** add workspace secrets and custom HTTP auth ([#77](https://github.com/haswalt/kasuri/issues/77)) ([621e8a6](https://github.com/haswalt/kasuri/commit/621e8a654cc5507b5b9f11b21def8373b5a61a50))
* **workflows:** add explicit failure termination ([#82](https://github.com/haswalt/kasuri/issues/82)) ([a3abd17](https://github.com/haswalt/kasuri/commit/a3abd1735d66b22c37c3700bdb697d5a32ae1243))


### Bug Fixes

* **clairic:** surface configuration and handled error diagnostics ([#81](https://github.com/haswalt/kasuri/issues/81)) ([5fdd27d](https://github.com/haswalt/kasuri/commit/5fdd27d5c095ac29df4aee6be0948b709cbe29da))
* **editor:** prevent duplicate input assembly and scope polling ([#69](https://github.com/haswalt/kasuri/issues/69)) ([a60252f](https://github.com/haswalt/kasuri/commit/a60252fd93e49eac1ab8ff20a6f5382a2294a2fa))
* **mcp:** preserve masked credentials on workflow updates ([#80](https://github.com/haswalt/kasuri/issues/80)) ([243b946](https://github.com/haswalt/kasuri/commit/243b946b3e7fc2ae32c0e1db7eddaf696b96e9e6))
* **mcp:** show exact scopes in OAuth consent ([#76](https://github.com/haswalt/kasuri/issues/76)) ([1cd52b4](https://github.com/haswalt/kasuri/commit/1cd52b42f1add2f298c7f5f4ad2b9bc06625dc5f))
* **workflows:** improve trash and hide deleted workflow runs ([#68](https://github.com/haswalt/kasuri/issues/68)) ([26bff9e](https://github.com/haswalt/kasuri/commit/26bff9e9a8a69a43757bf19305ab4a5bbc6a0dab))
