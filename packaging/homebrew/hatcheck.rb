# Homebrew formula for hatcheck. It goes in a tap, a repo named homebrew-tap: with this
# file at Formula/hatcheck.rb in github.com/yilab/homebrew-tap,
#   brew install yilab/tap/hatcheck
# installs a release, and brew install --HEAD yilab/tap/hatcheck the main branch.
# For each release, the url's tag and the sha256 of that tarball go here
# (shasum -a 256 of the downloaded tarball); the release workflow prints both.
class Hatcheck < Formula
  desc "Claude Code and Codex accounts on one computer, each kept apart"
  homepage "https://github.com/yilab/hatcheck"
  url "https://github.com/yilab/hatcheck/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  head "https://github.com/yilab/hatcheck.git", branch: "main"

  depends_on "python@3.13"

  def install
    libexec.install "hatcheck.py", "hatcheck", "cl", "cx"
    python = Formula["python@3.13"].opt_bin/"python3.13"
    # hatcheck and hat run with Homebrew's Python; hat names itself hat in its messages.
    (bin/"hatcheck").write <<~SH
      #!/bin/sh
      exec "#{python}" "#{libexec}/hatcheck.py" "$@"
    SH
    (bin/"hat").write <<~SH
      #!/bin/sh
      exec "#{python}" "#{libexec}/hatcheck.py" --as hat "$@"
    SH
    # cl and cx are sh scripts; linked as claude-z and claude-k, cl runs Claude Code on Z.ai or Kimi.
    bin.install_symlink libexec/"cl", libexec/"cx"
    bin.install_symlink libexec/"cl" => "claude-z"
    bin.install_symlink libexec/"cl" => "claude-k"
  end

  test do
    assert_match "hatcheck #{version}", shell_output("#{bin}/hatcheck --version") if build.stable?
    assert_match "hatcheck usage", shell_output("#{bin}/hatcheck --help")
    assert_match "usage: cl", shell_output("#{bin}/cl 2>&1", 2)
  end
end
