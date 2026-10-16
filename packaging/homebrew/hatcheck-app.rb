# Homebrew cask for the Hatcheck app (macOS, Apple silicon and Intel), from the
# desktop-v<version> GitHub release. It goes in the tap beside the formula, as
# Casks/hatcheck-app.rb:
#   brew install --cask yilab/tap/hatcheck-app
# For each release, set version, and the two sha256 values to `shasum -a 256` of that
# release's Hatcheck_<version>_aarch64.dmg and Hatcheck_<version>_x64.dmg.
cask "hatcheck-app" do
  arch arm: "aarch64", intel: "x64"

  version "0.1.0"
  sha256 arm:   "0000000000000000000000000000000000000000000000000000000000000000",
         intel: "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/yilab/hatcheck/releases/download/desktop-v#{version}/Hatcheck_#{version}_#{arch}.dmg"
  name "Hatcheck"
  desc "Claude and ChatGPT plans and recent sessions, in the menu bar"
  homepage "https://github.com/yilab/hatcheck"

  depends_on macos: ">= :big_sur"

  app "Hatcheck.app"

  zap trash: [
    "~/Library/Application Support/com.yilab.hatcheck",
    "~/Library/Caches/com.yilab.hatcheck",
    "~/Library/WebKit/com.yilab.hatcheck",
  ]
end
