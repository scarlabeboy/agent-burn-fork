type SyntheticSubject = Readonly<{
  id: 'windows' | 'selected-wsl' | 'computer';
  label: string;
  status: string;
}>;

type SyntheticShell = Readonly<{
  heading: string;
  banner: string;
  explanation: string;
  subjects: readonly SyntheticSubject[];
}>;

// Display fixtures only: these do not describe discovered environments or observations.
export const synthetic = Object.freeze({
  heading: 'Agent Burn Fork — Windows shell',
  banner: 'Synthetic placeholder — collection disabled',
  explanation: 'No provider usage, quota or spending has been observed.',
  subjects: Object.freeze([
    Object.freeze({ id: 'windows', label: 'Windows', status: 'Not connected' }),
    Object.freeze({ id: 'selected-wsl', label: 'Selected WSL', status: 'Not connected; no distro discovered' }),
    Object.freeze({ id: 'computer', label: 'Computer', status: 'Derived view only; no collection source' }),
  ] as const satisfies readonly SyntheticSubject[]),
} as const satisfies SyntheticShell);
