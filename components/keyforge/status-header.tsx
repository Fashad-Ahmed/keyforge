import type { AppInfo } from "@/lib/types/app-info";
import type { RuntimeInputStatus } from "@/lib/types/runtime";

const INPUT_LABELS: Record<RuntimeInputStatus, string> = {
  permission_denied: "INPUT PERMISSION REQUIRED",
  ready: "INPUT ENGINE READY",
  starting: "INPUT ENGINE STARTING",
  unavailable: "INPUT ENGINE UNAVAILABLE",
  unsupported: "INPUT ADAPTER UNSUPPORTED",
};

type StatusHeaderProps = {
  info?: AppInfo;
  inputStatus?: RuntimeInputStatus;
  nativeUnavailable: boolean;
};

export function StatusHeader({ info, inputStatus, nativeUnavailable }: StatusHeaderProps) {
  return (
    <header className="instrument-header">
      <div className="brand-lockup">
        <span aria-hidden="true" className="window-marks"><i /><i /><i /></span>
        <div><h1>KeyForge</h1><p>KEYFORGE / LOCAL</p></div>
      </div>
      <div className="header-meta">
        {nativeUnavailable ? <span>Native runtime unavailable</span> : info ? (
          <><span>Version: {info.version}</span><span>Platform: {info.platform}</span></>
        ) : <span>Connecting...</span>}
      </div>
      <div className={`engine-status engine-status-${inputStatus ?? "starting"}`}>
        <span aria-hidden="true" className="status-light" />
        <span>{INPUT_LABELS[inputStatus ?? "starting"]}</span>
      </div>
    </header>
  );
}
