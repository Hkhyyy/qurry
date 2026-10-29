# Homebrew formula template (Formula/qurry.rb in Hkhyyy/homebrew-qurry). At release, release.yml fills in {{…}} and pushes it to the tap
class Qurry < Formula
  desc "Free DataGrip alternative: terminal UI and MCP server for PostgreSQL, MySQL and Redis"
  homepage "https://github.com/Hkhyyy/qurry"
  version "{{VERSION}}"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/Hkhyyy/qurry/releases/download/v{{VERSION}}/qurry-cli_{{VERSION}}_aarch64-apple-darwin.tar.gz"
      sha256 "{{SHA_aarch64-apple-darwin}}"
    end
    on_intel do
      url "https://github.com/Hkhyyy/qurry/releases/download/v{{VERSION}}/qurry-cli_{{VERSION}}_x86_64-apple-darwin.tar.gz"
      sha256 "{{SHA_x86_64-apple-darwin}}"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/Hkhyyy/qurry/releases/download/v{{VERSION}}/qurry-cli_{{VERSION}}_x86_64-unknown-linux-gnu.tar.gz"
      sha256 "{{SHA_x86_64-unknown-linux-gnu}}"
    end
  end

  def install
    bin.install "qurry-tui", "qurry-mcp"
  end

  def caveats
    <<~EOS
      Register the MCP server with Claude Code (read-only by default):
        claude mcp add qurry -- #{opt_bin}/qurry-mcp
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/qurry-tui --version")
  end
end
