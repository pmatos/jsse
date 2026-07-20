/* Conventional Commits ruleset for commitlint.
 * .cjs so it loads correctly whether or not the repo's package.json sets "type":"module". */
module.exports = {
  extends: ['@commitlint/config-conventional'],
};
