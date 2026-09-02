# Optional dynamic account shortcuts for zsh. Source this file from a shell
# profile only when you want providerN names; installation never sources it.

_neomax_define_account_shortcut() {
  local function_name="$1" launcher="$2" mode="$3" account="$4"
  if [[ "$mode" == orchestrator ]]; then
    functions[$function_name]="command $launcher $account \"\$@\""
  else
    functions[$function_name]="command $launcher run $account \"\$@\""
  fi
}

_neomax_define_account_shortcut claude1 cmax orchestrator 1
for _neomax_profile in "$HOME"/.claude-acct<2->(Nn/); do
  _neomax_account="${_neomax_profile##*-acct}"
  _neomax_define_account_shortcut "claude${_neomax_account}" cmax orchestrator "${_neomax_account}"
done

_neomax_define_account_shortcut codex1 cdx helper 1
for _neomax_profile in "$HOME"/.codex-acct<2->(Nn/); do
  _neomax_account="${_neomax_profile##*-acct}"
  _neomax_define_account_shortcut "codex${_neomax_account}" cdx helper "${_neomax_account}"
done

_neomax_define_account_shortcut opencode1 ocx helper 1
for _neomax_profile in "$HOME"/.opencode-acct<2->(Nn/); do
  _neomax_account="${_neomax_profile##*-acct}"
  _neomax_define_account_shortcut "opencode${_neomax_account}" ocx helper "${_neomax_account}"
done

_neomax_define_account_shortcut kimi1 kmx helper 1
for _neomax_profile in "$HOME"/.kimi-code-acct<2->(Nn/); do
  _neomax_account="${_neomax_profile##*-acct}"
  _neomax_define_account_shortcut "kimi${_neomax_account}" kmx helper "${_neomax_account}"
done

_neomax_define_account_shortcut grok1 gmx helper 1
for _neomax_profile in "$HOME"/.grok-acct<2->(Nn/); do
  _neomax_account="${_neomax_profile##*-acct}"
  _neomax_define_account_shortcut "grok${_neomax_account}" gmx helper "${_neomax_account}"
done

unset _neomax_profile _neomax_account
unfunction _neomax_define_account_shortcut
