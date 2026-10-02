-- The receipt has already been found. A denied target must never become a new request.
SELECT *, payload.sealed AS sealed FROM ONLY $journal
WHERE id != NONE AND provider_instance_id = $provider
  AND computer_id = $computer_id AND actor_key = $actor_key
  AND binding.provider_instance_id = <string>$provider
  AND binding.computer_id = $computer_text AND binding.actor_key = $actor_key
  AND (IF $file { binding.required_labels; } ELSE { binding.required_output_labels; }) ALLINSIDE $labels;
