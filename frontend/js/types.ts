export interface FsNode {
    name: string;
    path: string;
    type: 'directory' | 'file';
    size: number;
    size_human: string;
    modified_time: string;
    created_time: string;
    readonly: boolean;
    extension?: string;
    children_count?: number;
    children?: FsNode[];
}

export interface PeerInfo {
    id: string;
    name: string;
    addr: string;
    port: number;
}

export interface TransferSession {
    id: string;
    code: string;
    file_path: string;
    filename: string;
    size: number;
    state: 'Pending' | 'Accepted' | 'Done' | 'Cancelled' | 'Rejected';
    sender_name: string;
    sender_addr: string;
    sender_port: number;
    created_at_secs: number;
}

export interface IncomingTransferPayload {
    session_id: string;
    code: string;
    filename: string;
    size: number;
    sender_name: string;
}

export interface TransferProgressPayload {
    session_id: string;
    bytes_done: number;
    total: number;
}

export interface TransferCompletePayload {
    session_id: string;
    saved_path: string;
}

export interface TransferErrorPayload {
    session_id: string;
    error: string;
}
