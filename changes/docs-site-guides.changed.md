The docs site is organised around what you are trying to do. A Guides section
walks through nine tasks (connect your agent, check before you edit, catch what
an agent broke, run only the affected tests, review a pull request, adopt on a
legacy repository, block breaking API changes, work across repositories,
understand a new codebase), each with the commands and real output. The
quickstart and the first-five-minutes page are now one page built on
`tessera init`; the old address redirects to it. The landing page opens with
a "What do you want to do?" chooser, maintainer pages sit under Contribute,
and the troubleshooting page has a short FAQ. The `docs-site` CI job now also
fails on a dead relative link.
