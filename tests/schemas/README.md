# JUnit schema

`junit-10.xsd` is copied unchanged from Jenkins xUnit plugin commit
`0afa700702a6617f483cfd9407711976dc0bd74a`:
https://github.com/jenkinsci/xunit-plugin/blob/0afa700702a6617f483cfd9407711976dc0bd74a/src/main/resources/org/jenkinsci/plugins/xunit/types/model/xsd/junit-10.xsd

SHA256: `a1a816f58d1bf95ebabf371994df0b9246dee66ea9572fbec4f9296f1b2c0ff6`.
The original MIT license is retained in the file. This schema accepts suite
properties, not testcase properties. Validation does not establish live CI
acceptance or retention of custom properties by every importer.
