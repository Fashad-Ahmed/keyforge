type OperationMessageProps = { message: { kind: "error" | "success"; text: string } | null };

export function OperationMessage({ message }: OperationMessageProps) {
  if (!message) return null;
  return <p className={`operation-message operation-message-${message.kind}`} role="status">{message.text}</p>;
}
