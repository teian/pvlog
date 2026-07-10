CREATE UNIQUE INDEX rbac_assignments_instance_unique_idx
    ON rbac_role_assignments(role_id, principal_type, principal_id, scope_type)
    WHERE scope_type = 'instance';

CREATE UNIQUE INDEX rbac_assignments_account_unique_idx
    ON rbac_role_assignments(role_id, principal_type, principal_id, scope_type, account_id)
    WHERE scope_type = 'account';

CREATE UNIQUE INDEX rbac_assignments_system_unique_idx
    ON rbac_role_assignments(role_id, principal_type, principal_id, scope_type, account_id, system_id)
    WHERE scope_type = 'system';
