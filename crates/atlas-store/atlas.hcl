// Atlas CLI configuration for atlas-store. Runtime code embeds SQL from the
// migrations directory; Atlas is only a development-time authoring tool.
env "local" {
  src = "file://schema.hcl"
  dev = "sqlite://atlas-financer-dev?mode=memory&_fk=1"

  migration {
    dir = "file://migrations"
  }
}
