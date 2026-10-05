SELECT * FROM uav_task WHERE task IN (SELECT VALUE task FROM $lookups WHERE plan_id IN $plan_ids)
    AND task IN (SELECT VALUE task FROM $executions WHERE plan IN $admitted_plans
        AND plan.plan_id = $parent.identity.plan_id);
