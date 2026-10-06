UPDATE $definition SET disabled = $disabled, status = IF $disabled THEN 'disabled' ELSE 'enabled' END RETURN NONE;
