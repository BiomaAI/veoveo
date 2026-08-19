import { identityLabel, type IdentityDirectory } from "../identity";

export function IdentityText({
  identity,
  directory,
  className,
}: {
  identity: string | undefined;
  directory: IdentityDirectory;
  className?: string;
}) {
  const label = identityLabel(identity, directory);
  return <span className={className}>{label}</span>;
}
