SELECT
  id,
  execution_id,
  transfer_id,
  computer_id,
  provider_instance_id,
  actor_key,
  binding,
  authority,
  task,
  stage
FROM ONLY $execution
WHERE
  provider_instance_id = $provider
  AND computer_id = $computer_id
  AND binding.owner_key = $owner_key
  AND (
    binding.grant_id ?? NONE
  ) = $grant_id
  AND (
    $owned = true
    OR actor_key = $actor_key
  )
  AND (
    IF $file {
      binding.required_labels;
    } ELSE {
      binding.required_output_labels;
    }
  ) ALLINSIDE $labels
  AND $admission_end > time::now();
