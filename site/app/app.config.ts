export default defineAppConfig({
  site: {
    name: "mm",
    url: "https://modmon.dev",
    description:
      "A Docker Swarm replacement: turn a list of servers into one cluster that runs your Docker Compose project.",
    // packaging/release/install.sh, which the site build copies to /install.sh.
    installUrl: "https://modmon.dev/install.sh",
    // Linked from the navigation and the footer.
    repo: "https://github.com/modmonhq/modmon",
    vented: "https://vented.com",
  },
});
